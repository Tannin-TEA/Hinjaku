use eframe::egui::{self, Color32, ScrollArea};
use crate::config;
use crate::manager::Manager;
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

/// メイン画像表示エリアの描画（マンガモード対応）
pub fn draw_main_area(
    ui: &mut egui::Ui,
    manager: &Manager,
    mode: DisplayMode,
    zoom: f32,
    manga_mode: bool,
    manga_rtl: bool,
    manga_shift: bool,
    ctx: &egui::Context,
) {
    let avail = ui.available_size();
    
    // 1枚目の取得
    let tex1_data = manager.get_tex(manager.current, ctx.input(|i| i.time));
    let (tex1, tex1_size) = match tex1_data {
        Some((t, _)) => (t, t.size_vec2()),
        None => return,
    };

    // 2枚目のペアリング判定
    let can_pair = (manga_shift || manager.current > 0) && tex1_size.x <= tex1_size.y;
    let tex2_data = if manga_mode && can_pair {
        manager.get_tex(manager.current + 1, ctx.input(|i| i.time)).and_then(|(t, _)| {
            if t.size_vec2().x <= t.size_vec2().y { Some((t, t.size_vec2())) } else { None }
        })
    } else {
        None
    };

    ScrollArea::both().show(ui, |ui| {
        if manga_mode {
            if let Some((tex2, tex2_size)) = tex2_data {
                // 2枚並べ計算
                let half = egui::vec2(avail.x / 2.0, avail.y);
                let s1 = match mode {
                    DisplayMode::Fit => (half.x/tex1_size.x).min(half.y/tex1_size.y).min(1.0),
                    DisplayMode::WindowFit => (half.x/tex1_size.x).min(half.y/tex1_size.y),
                    DisplayMode::Manual => zoom,
                };
                let s2 = match mode {
                    DisplayMode::Fit => (half.x/tex2_size.x).min(half.y/tex2_size.y).min(1.0),
                    DisplayMode::WindowFit => (half.x/tex2_size.x).min(half.y/tex2_size.y),
                    DisplayMode::Manual => zoom,
                };
                let ds1 = tex1_size * s1;
                let ds2 = tex2_size * s2;
                let total_w = (ds1.x + ds2.x).max(avail.x);
                let total_h = ds1.y.max(ds2.y).max(avail.y);
                
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(total_w, total_h), egui::Sense::click());
                let cx = rect.min.x + total_w / 2.0;
                let uv = egui::Rect::from_min_max(egui::pos2(0.0,0.0), egui::pos2(1.0,1.0));
                
                if manga_rtl {
                    let r1 = egui::Rect::from_min_size(egui::pos2(cx, rect.min.y+(total_h-ds1.y)/2.0), ds1);
                    let r2 = egui::Rect::from_min_size(egui::pos2(cx-ds2.x, rect.min.y+(total_h-ds2.y)/2.0), ds2);
                    ui.painter().image(tex1.id(), r1, uv, Color32::WHITE);
                    ui.painter().image(tex2.id(), r2, uv, Color32::WHITE);
                } else {
                    let r1 = egui::Rect::from_min_size(egui::pos2(cx-ds1.x, rect.min.y+(total_h-ds1.y)/2.0), ds1);
                    let r2 = egui::Rect::from_min_size(egui::pos2(cx, rect.min.y+(total_h-ds2.y)/2.0), ds2);
                    ui.painter().image(tex1.id(), r1, uv, Color32::WHITE);
                    ui.painter().image(tex2.id(), r2, uv, Color32::WHITE);
                }
                return resp;
            }
        }
        
        // 1枚のみ（または2枚目ロード中）
        draw_centered(ui, tex1.id(), tex1_size, avail, mode, zoom)
    }).inner
}