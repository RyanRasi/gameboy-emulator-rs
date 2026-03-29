//! Axum route handlers.
//!
//! Routes:
//!   POST /bios          Upload BIOS (raw bytes, optional)
//!   POST /rom           Upload ROM (raw bytes)
//!   POST /start         Begin emulation
//!   POST /frame         Advance one frame, return framebuffer as base64
//!   POST /press/:btn    Press a button
//!   POST /release/:btn  Release a button
//!   GET  /status        Current emulator status as JSON

use std::sync::{Arc, Mutex};

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

use gb_core::input::Button;

use crate::state::{EmulatorState, EmulatorStatus};

pub type SharedState = Arc<Mutex<EmulatorState>>;

// ── JSON response shapes ──────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct OkResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Serialize)]
pub struct FrameResponse {
    /// Base64-encoded framebuffer: 160×144 bytes, shade 0–3 per pixel.
    pub framebuffer: String,
    /// Total width in pixels.
    pub width:  usize,
    /// Total height in pixels.
    pub height: usize,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub status: String,
    pub has_rom: bool,
}

// ── Button name parsing ───────────────────────────────────────────────────────

pub fn parse_button(name: &str) -> Option<Button> {
    match name.to_lowercase().as_str() {
        "a"      => Some(Button::A),
        "b"      => Some(Button::B),
        "start"  => Some(Button::Start),
        "select" => Some(Button::Select),
        "up"     => Some(Button::Up),
        "down"   => Some(Button::Down),
        "left"   => Some(Button::Left),
        "right"  => Some(Button::Right),
        _        => None,
    }
}

fn ok(msg: impl Into<String>) -> Json<OkResponse> {
    Json(OkResponse { ok: true, message: Some(msg.into()) })
}

fn err_response(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<OkResponse>) {
    (status, Json(OkResponse { ok: false, message: Some(msg.into()) }))
}

// ── Route handlers ────────────────────────────────────────────────────────────

async fn post_bios(
    State(state): State<SharedState>,
    body: Bytes,
) -> Result<Json<OkResponse>, (StatusCode, Json<OkResponse>)> {
    state
        .lock()
        .unwrap()
        .upload_bios(body.to_vec())
        .map(|_| ok("BIOS uploaded"))
        .map_err(|e| err_response(StatusCode::BAD_REQUEST, e))
}

async fn post_rom(
    State(state): State<SharedState>,
    body: Bytes,
) -> Result<Json<OkResponse>, (StatusCode, Json<OkResponse>)> {
    state
        .lock()
        .unwrap()
        .upload_rom(body.to_vec())
        .map(|title| ok(format!("ROM loaded: {}", title)))
        .map_err(|e| err_response(StatusCode::BAD_REQUEST, e))
}

async fn post_start(
    State(state): State<SharedState>,
) -> Result<Json<OkResponse>, (StatusCode, Json<OkResponse>)> {
    state
        .lock()
        .unwrap()
        .start()
        .map(|_| ok("Emulator started"))
        .map_err(|e| err_response(StatusCode::CONFLICT, e))
}

async fn post_frame(
    State(state): State<SharedState>,
) -> Result<Json<FrameResponse>, (StatusCode, Json<OkResponse>)> {
    state
        .lock()
        .unwrap()
        .run_frame()
        .map(|fb| {
            Json(FrameResponse {
                framebuffer: B64.encode(&fb),
                width:  gb_core::ppu::SCREEN_WIDTH,
                height: gb_core::ppu::SCREEN_HEIGHT,
            })
        })
        .map_err(|e| err_response(StatusCode::CONFLICT, e))
}

async fn post_press(
    State(state): State<SharedState>,
    Path(btn): Path<String>,
) -> Result<Json<OkResponse>, (StatusCode, Json<OkResponse>)> {
    let button = parse_button(&btn)
        .ok_or_else(|| err_response(StatusCode::BAD_REQUEST, format!("Unknown button: {}", btn)))?;
    state
        .lock()
        .unwrap()
        .press(button)
        .map(|_| ok(format!("Pressed {}", btn)))
        .map_err(|e| err_response(StatusCode::CONFLICT, e))
}

async fn post_release(
    State(state): State<SharedState>,
    Path(btn): Path<String>,
) -> Result<Json<OkResponse>, (StatusCode, Json<OkResponse>)> {
    let button = parse_button(&btn)
        .ok_or_else(|| err_response(StatusCode::BAD_REQUEST, format!("Unknown button: {}", btn)))?;
    state
        .lock()
        .unwrap()
        .release(button)
        .map(|_| ok(format!("Released {}", btn)))
        .map_err(|e| err_response(StatusCode::CONFLICT, e))
}

async fn get_status(
    State(state): State<SharedState>,
) -> Json<StatusResponse> {
    let s = state.lock().unwrap();
    let status_str = match s.status {
        EmulatorStatus::NoRom   => "no_rom",
        EmulatorStatus::Ready   => "ready",
        EmulatorStatus::Running => "running",
    };
    Json(StatusResponse {
        status:  status_str.into(),
        has_rom: s.cpu.is_some(),
    })
}

// ── Router builder ────────────────────────────────────────────────────────────

pub fn build_router(state: SharedState) -> Router {
    Router::new()
        .route("/bios",            post(post_bios))
        .route("/rom",             post(post_rom))
        .route("/start",           post(post_start))
        .route("/frame",           post(post_frame))
        .route("/press/:btn",      post(post_press))
        .route("/release/:btn",    post(post_release))
        .route("/status",          get(get_status))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;

    fn make_rom(cart_type: u8) -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0147] = cart_type;
        rom[0x0148] = 0x00;
        rom[0x0149] = 0x00;
        let cs = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = cs;
        rom
    }

    fn test_server() -> TestServer {
        let state = Arc::new(Mutex::new(EmulatorState::new()));
        TestServer::new(build_router(state)).unwrap()
    }

    // ── /status ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_status_initial_is_no_rom() {
        let server = test_server();
        let resp = server.get("/status").await;
        resp.assert_status_ok();
        let body: serde_json::Value = resp.json();
        assert_eq!(body["status"], "no_rom");
        assert_eq!(body["has_rom"], false);
    }

    // ── /bios ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_bios_upload_correct_size_succeeds() {
        let server = test_server();
        let resp = server
            .post("/bios")
            .bytes(Bytes::from(vec![0u8; 256]))
            .await;
        resp.assert_status_ok();
        let body: serde_json::Value = resp.json();
        assert_eq!(body["ok"], true);
    }

    #[tokio::test]
    async fn test_bios_upload_wrong_size_fails() {
        let server = test_server();
        let resp = server
            .post("/bios")
            .bytes(Bytes::from(vec![0u8; 512]))
            .await;
        resp.assert_status(StatusCode::BAD_REQUEST);
        let body: serde_json::Value = resp.json();
        assert_eq!(body["ok"], false);
    }

    // ── /rom ──────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_rom_upload_valid_succeeds() {
        let server = test_server();
        let resp = server
            .post("/rom")
            .bytes(Bytes::from(make_rom(0x00)))
            .await;
        resp.assert_status_ok();
        let body: serde_json::Value = resp.json();
        assert_eq!(body["ok"], true);
    }

    #[tokio::test]
    async fn test_rom_upload_invalid_returns_400() {
        let server = test_server();
        let resp = server
            .post("/rom")
            .bytes(Bytes::from(vec![0u8; 10]))
            .await;
        resp.assert_status(StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_rom_upload_updates_status_to_ready() {
        let server = test_server();
        server.post("/rom").bytes(Bytes::from(make_rom(0x00))).await;
        let resp = server.get("/status").await;
        let body: serde_json::Value = resp.json();
        assert_eq!(body["status"], "ready");
        assert_eq!(body["has_rom"], true);
    }

    // ── /start ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_start_without_rom_returns_409() {
        let server = test_server();
        let resp = server.post("/start").await;
        resp.assert_status(StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_start_after_rom_upload_succeeds() {
        let server = test_server();
        server.post("/rom").bytes(Bytes::from(make_rom(0x00))).await;
        let resp = server.post("/start").await;
        resp.assert_status_ok();
        let body: serde_json::Value = resp.json();
        assert_eq!(body["ok"], true);
    }

    #[tokio::test]
    async fn test_start_sets_status_to_running() {
        let server = test_server();
        server.post("/rom").bytes(Bytes::from(make_rom(0x00))).await;
        server.post("/start").await;
        let resp = server.get("/status").await;
        let body: serde_json::Value = resp.json();
        assert_eq!(body["status"], "running");
    }

    #[tokio::test]
    async fn test_start_is_idempotent() {
        let server = test_server();
        server.post("/rom").bytes(Bytes::from(make_rom(0x00))).await;
        server.post("/start").await;
        let resp = server.post("/start").await;
        resp.assert_status_ok();
    }

    // ── /frame ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_frame_before_start_returns_409() {
        let server = test_server();
        server.post("/rom").bytes(Bytes::from(make_rom(0x00))).await;
        let resp = server.post("/frame").await;
        resp.assert_status(StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_frame_returns_valid_response() {
        let server = test_server();
        server.post("/rom").bytes(Bytes::from(make_rom(0x00))).await;
        server.post("/start").await;
        let resp = server.post("/frame").await;
        resp.assert_status_ok();
        let body: serde_json::Value = resp.json();
        assert!(body["framebuffer"].is_string());
        assert_eq!(body["width"],  160);
        assert_eq!(body["height"], 144);
    }

    #[tokio::test]
    async fn test_frame_framebuffer_is_valid_base64() {
        let server = test_server();
        server.post("/rom").bytes(Bytes::from(make_rom(0x00))).await;
        server.post("/start").await;
        let resp = server.post("/frame").await;
        let body: serde_json::Value = resp.json();
        let b64 = body["framebuffer"].as_str().unwrap();
        let decoded = B64.decode(b64).unwrap();
        assert_eq!(decoded.len(), gb_core::ppu::FRAMEBUFFER_SIZE);
    }

    #[tokio::test]
    async fn test_frame_framebuffer_contains_valid_shades() {
        let server = test_server();
        server.post("/rom").bytes(Bytes::from(make_rom(0x00))).await;
        server.post("/start").await;
        let resp = server.post("/frame").await;
        let body: serde_json::Value = resp.json();
        let b64 = body["framebuffer"].as_str().unwrap();
        let decoded = B64.decode(b64).unwrap();
        assert!(decoded.iter().all(|&s| s <= 3), "All shades must be 0–3");
    }

    #[tokio::test]
    async fn test_multiple_frames_succeed() {
        let server = test_server();
        server.post("/rom").bytes(Bytes::from(make_rom(0x00))).await;
        server.post("/start").await;
        for _ in 0..5 {
            let resp = server.post("/frame").await;
            resp.assert_status_ok();
        }
    }

    // ── /press and /release ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_press_valid_button_succeeds() {
        let server = test_server();
        server.post("/rom").bytes(Bytes::from(make_rom(0x00))).await;
        server.post("/start").await;
        let resp = server.post("/press/a").await;
        resp.assert_status_ok();
        let body: serde_json::Value = resp.json();
        assert_eq!(body["ok"], true);
    }

    #[tokio::test]
    async fn test_press_invalid_button_returns_400() {
        let server = test_server();
        server.post("/rom").bytes(Bytes::from(make_rom(0x00))).await;
        server.post("/start").await;
        let resp = server.post("/press/turbo").await;
        resp.assert_status(StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_press_before_start_returns_409() {
        let server = test_server();
        server.post("/rom").bytes(Bytes::from(make_rom(0x00))).await;
        let resp = server.post("/press/a").await;
        resp.assert_status(StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_release_valid_button_succeeds() {
        let server = test_server();
        server.post("/rom").bytes(Bytes::from(make_rom(0x00))).await;
        server.post("/start").await;
        server.post("/press/start").await;
        let resp = server.post("/release/start").await;
        resp.assert_status_ok();
    }

    #[tokio::test]
    async fn test_all_buttons_accepted() {
        let server = test_server();
        server.post("/rom").bytes(Bytes::from(make_rom(0x00))).await;
        server.post("/start").await;
        for btn in ["a","b","start","select","up","down","left","right"] {
            let resp = server.post(&format!("/press/{}", btn)).await;
            resp.assert_status_ok();
        }
    }

    // ── parse_button ──────────────────────────────────────────────────────────

    #[test]
    fn test_parse_button_all_valid_names() {
        for (name, expected) in [
            ("a",      Button::A),
            ("b",      Button::B),
            ("start",  Button::Start),
            ("select", Button::Select),
            ("up",     Button::Up),
            ("down",   Button::Down),
            ("left",   Button::Left),
            ("right",  Button::Right),
        ] {
            assert_eq!(parse_button(name), Some(expected), "Failed for '{}'", name);
        }
    }

    #[test]
    fn test_parse_button_case_insensitive() {
        assert_eq!(parse_button("A"),     Some(Button::A));
        assert_eq!(parse_button("START"), Some(Button::Start));
        assert_eq!(parse_button("Up"),    Some(Button::Up));
    }

    #[test]
    fn test_parse_button_invalid_returns_none() {
        assert_eq!(parse_button("turbo"),  None);
        assert_eq!(parse_button(""),       None);
        assert_eq!(parse_button("select2"),None);
    }
}