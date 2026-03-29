//! Game Boy Emulator — Web Server
//!
//! Usage: web [--port PORT]   (default: 3000)
//!
//! Example workflow:
//!   curl -X POST http://localhost:3000/rom  --data-binary @Tetris.gb
//!   curl -X POST http://localhost:3000/start
//!   curl -X POST http://localhost:3000/frame   # returns base64 framebuffer
//!   curl -X POST http://localhost:3000/press/start
//!   curl -X POST http://localhost:3000/release/start

mod server;
mod state;

use std::sync::{Arc, Mutex};
use state::EmulatorState;

#[tokio::main]
async fn main() {
    env_logger::init();

    let port = std::env::args()
        .skip_while(|a| a != "--port")
        .nth(1)
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(3000);

    let shared = Arc::new(Mutex::new(EmulatorState::new()));
    let app    = server::build_router(shared);

    let addr = format!("0.0.0.0:{}", port);
    log::info!("Game Boy web server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}