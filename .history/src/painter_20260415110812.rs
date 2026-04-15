use eframe::egui::{self, Color32};
use crate::config;
use crate::viewer::DisplayMode;

/// 市松模様のタイルサイズ
pub const CHECKERBOARD_GRID_SIZE: f32 = 16.0;
/// 市松模様の色1 (暗い)
pub const CHECKERBOARD_COLOR_1: Color32 = Color32::from_gray(25);
/// 市松模様の色2 (明るい)
pub const CHECKERBOARD_COLOR_2: Color32 = Color32::from_gray(40);

pub fn paint_background(ui: &mut egui::Ui, rect: egui::Rect, mode: config::BackgroundMode) {
    match mode {
        config::BackgroundMode::Theme => {}, // 何もしない（eguiのパネル色を使用）
        config::BackgroundMode::Black => { ui.painter().rect_filled(rect, 0.0, Color32::BLACK); },
        config::BackgroundMode::Gray => { ui.painter().rect_filled(rect, 0.0, Color32::from_gray(64)); },
        config::BackgroundMode::White => { ui.painter().rect_filled(rect, 0.0, Color32::WHITE); },
        config::BackgroundMode::Checkerboard => {
            // 下地
            ui.painter().rect_filled(rect, 0.0, CHECKERBOARD_COLOR_1);
            
            let mut gx = (rect.min.x / CHECKERBOARD_GRID_SIZE).floor();
            while gx * CHECKERBOARD_GRID_SIZE < rect.max.x {
                let mut gy = (rect.min.y / CHECKERBOARD_GRID_SIZE).floor();
                while gy * CHECKERBOARD_GRID_SIZE < rect.max.y {
                    if (gx as i32 + gy as i32) % 2 == 0 {
                        let tile = egui::Rect::from_min_size(
                            egui::pos2(gx * CHECKERBOARD_GRID_SIZE, gy * CHECKERBOARD_GRID_SIZE),
                            egui::vec2(CHECKERBOARD_GRID_SIZE, CHECKERBOARD_GRID_SIZE)
                        ).intersect(rect);
                        if !tile.is_negative() {
                            ui.painter().rect_filled(tile, 0.0, CHECKERBOARD_COLOR_2);
                        }
                    }
                    gy += 1.0;
                }
                gx += 1.0;
            }
        }
    }
}

pub fn draw_centered(
    ui: &mut egui::Ui,
    tex_id: egui::TextureId,
    tex_size: egui::Vec2,
    avail: egui::Vec2,
    mode: DisplayMode,
    zoom: f32,
) -> egui::Response {
    let display_size = match mode {
        DisplayMode::Fit => {
            let scale = (avail.x / tex_size.x).min(avail.y / tex_size.y).min(1.0);
            tex_size * scale
        }
        DisplayMode::WindowFit => {
            let scale = (avail.x / tex_size.x).min(avail.y / tex_size.y);
            tex_size * scale
        }
        DisplayMode::Manual => tex_size * zoom,
    };
    let area = egui::vec2(display_size.x.max(avail.x), display_size.y.max(avail.y));
    let off  = egui::vec2(((area.x - display_size.x)*0.5).max(0.0), ((area.y - display_size.y)*0.5).max(0.0));
    let (rect, resp) = ui.allocate_exact_size(area, egui::Sense::click());
    let img_rect = egui::Rect::from_min_size(rect.min + off, display_size);
    let uv = egui::Rect::from_min_max(egui::pos2(0.0,0.0), egui::pos2(1.0,1.0));
    ui.painter().image(tex_id, img_rect, uv, egui::Color32::WHITE);
    resp
}