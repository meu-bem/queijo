//! The official Queijo icon, embedded in the binary.

use eframe::egui;
use image::RgbaImage;

const PNG: &[u8] = include_bytes!("../assets/queijo-512.png");

fn decode() -> RgbaImage {
    image::load_from_memory(PNG).expect("embedded icon is a valid PNG").to_rgba8()
}

/// For the window's title bar and task switcher.
pub fn window_icon() -> egui::IconData {
    let img = decode();
    egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }
}

/// Pre-scaled so it stays crisp at small sizes (egui's own downscaling is rough).
pub fn texture(ctx: &egui::Context, size: u32) -> egui::TextureHandle {
    let img = image::imageops::resize(&decode(), size, size, image::imageops::FilterType::Lanczos3);
    let color = egui::ColorImage::from_rgba_unmultiplied([size as usize; 2], img.as_raw());
    ctx.load_texture("queijo-icon", color, egui::TextureOptions::LINEAR)
}
