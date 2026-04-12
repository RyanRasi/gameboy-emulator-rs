//! CGB Color Palette RAM and RGB555 → RGB888 conversion.

use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CgbPalette {
    pub ram:  Vec<u8>,
    pub spec: u8,
}

impl CgbPalette {
    pub fn new() -> Self {
        CgbPalette { ram: vec![0xFF; 64], spec: 0x00 }
    }

    pub fn index(&self) -> usize { (self.spec & 0x3F) as usize }
    pub fn auto_increment(&self) -> bool { self.spec & 0x80 != 0 }

    pub fn write_data(&mut self, value: u8) {
        let idx = self.index();
        if idx < 64 { self.ram[idx] = value; }
        if self.auto_increment() {
            self.spec = (self.spec & 0x80) | ((self.spec + 1) & 0x3F);
        }
    }

    pub fn read_data(&self) -> u8 {
        let idx = self.index();
        if idx < 64 { self.ram[idx] } else { 0xFF }
    }

    pub fn get_color(&self, pal: usize, color: usize) -> u32 {
        let off = (pal * 8 + color * 2).min(62);
        let lo  = self.ram[off]     as u16;
        let hi  = self.ram[off + 1] as u16;
        rgb555_to_rgb888(lo | (hi << 8))
    }
}

impl Default for CgbPalette { fn default() -> Self { Self::new() } }

pub fn rgb555_to_rgb888(rgb555: u16) -> u32 {
    let r5 = (rgb555        & 0x1F) as u32;
    let g5 = ((rgb555 >> 5) & 0x1F) as u32;
    let b5 = ((rgb555 >> 10)& 0x1F) as u32;
    ((r5 << 3 | r5 >> 2) << 16) | ((g5 << 3 | g5 >> 2) << 8) | (b5 << 3 | b5 >> 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb555_white()  { assert_eq!(rgb555_to_rgb888(0x7FFF), 0x00FFFFFF); }
    #[test]
    fn test_rgb555_black()  { assert_eq!(rgb555_to_rgb888(0x0000), 0x00000000); }
    #[test]
    fn test_rgb555_red()    { assert_eq!(rgb555_to_rgb888(0x001F), 0x00FF0000); }
    #[test]
    fn test_rgb555_green()  { assert_eq!(rgb555_to_rgb888(0x03E0), 0x0000FF00); }
    #[test]
    fn test_rgb555_blue()   { assert_eq!(rgb555_to_rgb888(0x7C00), 0x000000FF); }

    #[test]
    fn test_write_read_data() {
        let mut p = CgbPalette::new();
        p.spec = 0x00;
        p.write_data(0xAB);
        p.spec = 0x00;
        assert_eq!(p.read_data(), 0xAB);
    }

    #[test]
    fn test_auto_increment() {
        let mut p = CgbPalette::new();
        p.spec = 0x80;
        p.write_data(0x11);
        assert_eq!(p.index(), 1);
        p.write_data(0x22);
        assert_eq!(p.index(), 2);
    }

    #[test]
    fn test_get_color_white() {
        let mut p = CgbPalette::new();
        p.ram[0] = 0xFF; p.ram[1] = 0x7F;
        assert_eq!(p.get_color(0, 0), 0x00FFFFFF);
    }

    #[test]
    fn test_get_color_black() {
        let mut p = CgbPalette::new();
        p.ram[0] = 0x00; p.ram[1] = 0x00;
        assert_eq!(p.get_color(0, 0), 0x00000000);
    }
}