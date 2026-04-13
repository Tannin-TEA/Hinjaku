// ── 回転 ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Default)]
pub enum Rotation {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    pub fn cw(self) -> Self {
        match self {
            Self::R0 => Self::R90,
            Self::R90 => Self::R180,
            Self::R180 => Self::R270,
            Self::R270 => Self::R0,
        }
    }
    pub fn ccw(self) -> Self {
        match self {
            Self::R0 => Self::R270,
            Self::R90 => Self::R0,
            Self::R180 => Self::R90,
            Self::R270 => Self::R180,
        }
    }
}

// ── 画像変換 ─────────────────────────────────────────────────────────────────

/// 縦横いずれかが max_dim を超えたら縮小する
pub fn downscale_if_needed(
    img: image::RgbaImage,
    max_dim: u32,
    linear: bool,
) -> image::RgbaImage {
    let (w, h) = (img.width(), img.height());
    if w <= max_dim && h <= max_dim {
        return img;
    }
    let scale = (max_dim as f32 / w as f32).min(max_dim as f32 / h as f32);
    let nw = ((w as f32 * scale) as u32).max(1);
    let nh = ((h as f32 * scale) as u32).max(1);
    if linear {
        // thumbnail は resize より高速でダウンスケール品質も良好
        image::imageops::thumbnail(&img, nw, nh)
    } else {
        image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Nearest)
    }
}

pub fn apply_rotation(img: image::RgbaImage, rot: Rotation) -> image::RgbaImage {
    match rot {
        Rotation::R0 => img,
        Rotation::R90 => image::imageops::rotate90(&img),
        Rotation::R180 => image::imageops::rotate180(&img),
        Rotation::R270 => image::imageops::rotate270(&img),
    }
}

// ── egui 描画補助 ─────────────────────────────────────────────────────────────

/// 画像を利用可能領域の中央に描画し、インタラクション用のレスポンスを返す
pub fn draw_centered(
    ui: &mut eframe::egui::Ui,
    tex_id: eframe::egui::TextureId,
    tex_size: eframe::egui::Vec2,
    avail: eframe::egui::Vec2,
    fit: bool,
    zoom: f32,
) -> eframe::egui::Response {
    let display_size = if fit {
        let scale = (avail.x / tex_size.x)
            .min(avail.y / tex_size.y)
            .min(1.0);
        tex_size * scale
    } else {
        tex_size * zoom
    };
    let area = eframe::egui::vec2(
        display_size.x.max(avail.x),
        display_size.y.max(avail.y),
    );
    let off = eframe::egui::vec2(
        ((area.x - display_size.x) * 0.5).max(0.0),
        ((area.y - display_size.y) * 0.5).max(0.0),
    );
    let (rect, resp) = ui.allocate_exact_size(area, eframe::egui::Sense::click());
    let img_rect = eframe::egui::Rect::from_min_size(rect.min + off, display_size);
    let uv = eframe::egui::Rect::from_min_max(
        eframe::egui::pos2(0.0, 0.0),
        eframe::egui::pos2(1.0, 1.0),
    );
    ui.painter()
        .image(tex_id, img_rect, uv, eframe::egui::Color32::WHITE);
    resp
}
