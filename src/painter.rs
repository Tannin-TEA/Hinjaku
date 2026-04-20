use eframe::egui::{self, Color32, ScrollArea, Response, RichText};
use crate::config;
use crate::manager::Manager;
use crate::types::{DisplayMode, ViewState};
use crate::widgets::ViewerAction;
use crate::constants::*;

pub fn paint_background(ui: &mut egui::Ui, rect: egui::Rect, mode: config::BackgroundMode) {
    match mode {
        config::BackgroundMode::Theme => {}, // 何もしない（eguiのパネル色を使用）
        config::BackgroundMode::Black => { ui.painter().rect_filled(rect, 0.0, Color32::BLACK); },
        config::BackgroundMode::Gray => { ui.painter().rect_filled(rect, 0.0, Color32::from_gray(64)); },
        config::BackgroundMode::White => { ui.painter().rect_filled(rect, 0.0, Color32::WHITE); },
        config::BackgroundMode::Green => { ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(0, 64, 0)); },
        config::BackgroundMode::Checkerboard => {
            // 下地
            ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(0x19, 0x19, 0x19));
            
            let mut gx = (rect.min.x / painter::CHECKERBOARD_GRID_SIZE).floor();
            while gx * painter::CHECKERBOARD_GRID_SIZE < rect.max.x {
                let mut gy = (rect.min.y / painter::CHECKERBOARD_GRID_SIZE).floor();
                while gy * painter::CHECKERBOARD_GRID_SIZE < rect.max.y {
                    if (gx as i32 + gy as i32) % 2 == 0 {
                        let tile = egui::Rect::from_min_size(
                            egui::pos2(gx * painter::CHECKERBOARD_GRID_SIZE, gy * painter::CHECKERBOARD_GRID_SIZE),
                            egui::vec2(painter::CHECKERBOARD_GRID_SIZE, painter::CHECKERBOARD_GRID_SIZE)
                        ).intersect(rect);
                        if !tile.is_negative() {
                            ui.painter().rect_filled(tile, 0.0, Color32::from_rgb(0x28, 0x28, 0x28));
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
    let scale = match mode {
        DisplayMode::Fit => (avail.x / tex_size.x).min(avail.y / tex_size.y).min(1.0) * zoom,
        DisplayMode::WindowFit => (avail.x / tex_size.x).min(avail.y / tex_size.y) * zoom,
        DisplayMode::Manual => zoom,
    };
    let display_size = tex_size * scale;
    let area = egui::vec2(display_size.x.max(avail.x), display_size.y.max(avail.y));
    let off  = egui::vec2(((area.x - display_size.x)*0.5).max(0.0), ((area.y - display_size.y)*0.5).max(0.0));
    let (rect, resp) = ui.allocate_exact_size(area, egui::Sense::drag());
    if resp.dragged() {
        ui.scroll_with_delta(resp.drag_delta());
    }
    let img_rect = egui::Rect::from_min_size(rect.min + off, display_size);
    let uv = egui::Rect::from_min_max(egui::pos2(0.0,0.0), egui::pos2(1.0,1.0));
    ui.painter().image(tex_id, img_rect, uv, egui::Color32::WHITE);
    resp
}

/// メイン画像表示エリアの描画（マンガモード対応）
///
/// ⚠️ ここは描写の核心部。以下を変更する前にユーザーへ確認すること：
/// - ScrollArea の種類・設定（repaint の頻度に影響する）
/// - Sense の種類（drag / click / hover）の変更
/// - 画像配置の計算ロジック（マンガモードのページ並び順を含む）
pub fn draw_main_area(
    ui: &mut egui::Ui,
    manager: &Manager,
    view: &ViewState,
    manga_rtl: bool,
    ctx: &egui::Context,
    is_at_end: bool,
    secondary_down: bool,
    pending_scroll: Option<egui::Vec2>,
) -> (Response, Option<ViewerAction>, f32, egui::Vec2, egui::Pos2) {
    let mode       = view.display_mode;
    let zoom       = view.zoom;
    let manga_mode = view.manga_mode;
    let manga_shift = view.manga_shift;
    let avail = ui.available_size();
    let mut action = None;
    
    // 1枚目の取得
    let tex1_data = manager.get_tex(manager.current, ctx.input(|i| i.time));
    let (tex1, tex1_size) = match tex1_data {
        Some((t, next)) => {
            if let Some(secs) = next {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(secs));
            }
            (t, t.size_vec2())
        }
        None => return (ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover()), None, 1.0, egui::Vec2::ZERO, egui::Pos2::ZERO),
    };

    // 2枚目のペアリング判定
    let can_pair = (manga_shift || manager.current > 0) && tex1_size.x <= tex1_size.y;
    let tex2_data = if manga_mode && can_pair {
        manager.get_tex(manager.current + 1, ctx.input(|i| i.time)).and_then(|(t, next)| {
            if let Some(secs) = next {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(secs));
            }
            if t.size_vec2().x <= t.size_vec2().y { Some((t, t.size_vec2())) } else { None }
        })
    } else {
        None
    };

    let mut sa = ScrollArea::both().enable_scrolling(!secondary_down);
    if let Some(offset) = pending_scroll {
        sa = sa.scroll_offset(offset);
    }
    let output = sa.show(ui, |ui| {
        let current_eff;
        if manga_mode {
            if let Some((tex2, tex2_size)) = tex2_data {
                // 2枚並べ計算
                let half = egui::vec2(avail.x / 2.0, avail.y);
                let s1 = match mode {
                    DisplayMode::Fit => (half.x/tex1_size.x).min(half.y/tex1_size.y).min(1.0) * zoom,
                    DisplayMode::WindowFit => (half.x/tex1_size.x).min(half.y/tex1_size.y) * zoom,
                    DisplayMode::Manual => zoom,
                };
                let s2 = match mode {
                    DisplayMode::Fit => (half.x/tex2_size.x).min(half.y/tex2_size.y).min(1.0) * zoom,
                    DisplayMode::WindowFit => (half.x/tex2_size.x).min(half.y/tex2_size.y) * zoom,
                    DisplayMode::Manual => zoom,
                };
                current_eff = s1.min(s2);
                let ds1 = tex1_size * s1;
                let ds2 = tex2_size * s2;
                let total_w = (ds1.x + ds2.x).max(avail.x);
                let total_h = ds1.y.max(ds2.y).max(avail.y);
                
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(total_w, total_h), egui::Sense::drag());
                if resp.dragged() {
                    ui.scroll_with_delta(resp.drag_delta());
                }
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

                if is_at_end {
                    ui.add_space(20.0);
                    ui.vertical_centered(|ui| {
                        if ui.button(RichText::new("次のフォルダへ").size(20.0).strong()).clicked() {
                            action = Some(ViewerAction::NextDir);
                        }
                    });
                }
                return (resp, current_eff);
            }
        }
        
        // 1枚のみ（または2枚目ロード中）
        current_eff = match mode {
            DisplayMode::Fit => (avail.x / tex1_size.x).min(avail.y / tex1_size.y).min(1.0) * zoom,
            DisplayMode::WindowFit => (avail.x / tex1_size.x).min(avail.y / tex1_size.y) * zoom,
            DisplayMode::Manual => zoom,
        };
        let r = draw_centered(ui, tex1.id(), tex1_size, avail, mode, zoom);
        if is_at_end {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                if ui.button(RichText::new("次のフォルダへ").size(20.0).strong()).clicked() {
                    action = Some(ViewerAction::NextDir);
                }
            });
        }
        (r, current_eff)
    });
    let (resp, calculated_zoom) = output.inner;
    let scroll_off = output.state.offset;
    let vp_origin  = output.inner_rect.min;

    (resp, action, calculated_zoom, scroll_off, vp_origin)
}