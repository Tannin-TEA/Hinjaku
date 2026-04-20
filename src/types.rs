pub use crate::config::DisplayMode;

/// 描画・表示に関わる状態をまとめた構造体
#[derive(Clone)]
pub struct ViewState {
    pub display_mode: DisplayMode,
    pub zoom: f32,
    pub manga_mode: bool,
    pub manga_shift: bool,
    pub is_maximized: bool,
    pub is_fullscreen: bool,
    pub is_small_borderless: bool,
    pub effective_zoom: f32,
}

impl ViewState {
    pub fn new() -> Self {
        Self {
            display_mode: DisplayMode::Fit,
            zoom: 1.0,
            manga_mode: false,
            manga_shift: false,
            is_maximized: false,
            is_fullscreen: false,
            is_small_borderless: false,
            effective_zoom: 1.0,
        }
    }
}
