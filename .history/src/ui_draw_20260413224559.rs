use eframe::egui;
use crate::App;

pub fn draw_centered(
    ui: &mut egui::Ui,
    tex_id: egui::TextureId,
    tex_size: egui::Vec2,
    avail: egui::Vec2,
    fit: bool,
    zoom: f32,
) -> egui::Response {
    let display_size = if fit {
        let scale = (avail.x / tex_size.x)
            .min(avail.y / tex_size.y)
            .min(1.0);
        tex_size * scale
    } else {
        tex_size * zoom
    };
    let area = egui::vec2(
        display_size.x.max(avail.x),
        display_size.y.max(avail.y),
    );
    let off = egui::vec2(
        ((area.x - display_size.x) * 0.5).max(0.0),
        ((area.y - display_size.y) * 0.5).max(0.0),
    );
    let (rect, resp) = ui.allocate_exact_size(area, egui::Sense::click());
    let img_rect = egui::Rect::from_min_size(rect.min + off, display_size);
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    ui.painter()
        .image(tex_id, img_rect, uv, egui::Color32::WHITE);
    resp
}

pub fn draw_manga_pair(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    click_allowed: bool,
    app: &mut App,
    tex1_id: egui::TextureId,
    tex1_size: egui::Vec2,
    tex2_id: egui::TextureId,
    tex2_size: egui::Vec2,
    avail: egui::Vec2,
    fit: bool,
    zoom: f32,
) {
    let half = egui::vec2(avail.x / 2.0, avail.y);
    let s1 = if fit {
        (half.x / tex1_size.x)
            .min(half.y / tex1_size.y)
            .min(1.0)
    } else {
        zoom
    };
    let s2 = if fit {
        (half.x / tex2_size.x)
            .min(half.y / tex2_size.y)
            .min(1.0)
    } else {
        zoom
    };
    let ds1 = tex1_size * s1;
    let ds2 = tex2_size * s2;
    let total_w = (ds1.x + ds2.x).max(avail.x);
    let total_h = ds1.y.max(ds2.y).max(avail.y);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(total_w, total_h), egui::Sense::click());
    let cx = rect.min.x + total_w / 2.0;
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

    if app.config.manga_rtl {
        ui.painter().image(
            tex1_id,
            egui::Rect::from_min_size(
                egui::pos2(cx, rect.min.y + (total_h - ds1.y) / 2.0),
                ds1,
            ),
            uv,
            egui::Color32::WHITE,
        );
        ui.painter().image(
            tex2_id,
            egui::Rect::from_min_size(
                egui::pos2(cx - ds2.x, rect.min.y + (total_h - ds2.y) / 2.0),
                ds2,
            ),
            uv,
            egui::Color32::WHITE,
        );
    } else {
        ui.painter().image(
            tex1_id,
            egui::Rect::from_min_size(
                egui::pos2(cx - ds1.x, rect.min.y + (total_h - ds1.y) / 2.0),
                ds1,
            ),
            uv,
            egui::Color32::WHITE,
        );
        ui.painter().image(
            tex2_id,
            egui::Rect::from_min_size(
                egui::pos2(cx, rect.min.y + (total_h - ds2.y) / 2.0),
                ds2,
            ),
            uv,
            egui::Color32::WHITE,
        );
    }
    if click_allowed && resp.secondary_clicked() {
        app.go_prev(ctx);
    } else if click_allowed && resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let mut is_left = pos.x < rect.center().x;
            if app.config.manga_rtl {
                is_left = !is_left;
            }
            if is_left {
                app.go_prev(ctx);
            } else {
                app.go_next(ctx);
            }
        }
    }
}

pub fn draw_single_page(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    click_allowed: bool,
    app: &mut App,
    tex1_id: egui::TextureId,
    tex1_size: egui::Vec2,
    avail: egui::Vec2,
    fit: bool,
    zoom: f32,
) {
    let resp = draw_centered(ui, tex1_id, tex1_size, avail, fit, zoom);
    if click_allowed && resp.secondary_clicked() {
        app.go_prev(ctx);
    } else if click_allowed && resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let mut is_left = pos.x < resp.rect.center().x;
            if app.config.manga_rtl {
                is_left = !is_left;
            }
            if is_left {
                app.go_prev(ctx);
            } else {
                app.go_next(ctx);
            }
        }
    }
}
