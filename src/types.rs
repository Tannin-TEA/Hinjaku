/// 画像の表示モード
#[derive(PartialEq, Copy, Clone)]
pub enum DisplayMode {
    /// 画像が大きい場合のみ縮小（最大1.0倍）
    Fit,
    /// ウィンドウに合わせて拡大縮小（1.0倍を超えて拡大）
    WindowFit,
    /// ズーム倍率に基づく表示（100%など）
    Manual,
}

/// 描画・表示に関わる状態をまとめた構造体
#[derive(Clone)]
pub struct ViewState {
    pub display_mode: DisplayMode,
    pub zoom: f32,
    pub manga_mode: bool,
    pub manga_shift: bool,
    pub is_fullscreen: bool,
    pub is_borderless: bool,
}

impl ViewState {
    pub fn new() -> Self {
        Self {
            display_mode: DisplayMode::Fit,
            zoom: 1.0,
            manga_mode: false,
            manga_shift: false,
            is_fullscreen: false,
            is_borderless: false,
        }
    }
}
