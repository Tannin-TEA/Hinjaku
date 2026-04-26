#[derive(PartialEq, Copy, Clone, Debug)]
pub enum DisplayMode {
    Fit,
    WindowFit,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMode {
    /// 標準Mode (ウィンドウ枠あり、メニュー常駐表示)
    Standard,
    /// ボーダレスMode (ウィンドウ枠なし、メニュー隠し/オーバーレイ)
    Borderless,
    /// フルスクリーンMode (全画面表示、メニュー隠し/オーバーレイ)
    Fullscreen,
}

/// 描画・表示に関わる状態をまとめた構造体
#[derive(Clone)]
pub struct ViewState {
    pub display_mode: DisplayMode,
    pub zoom: f32,
    pub manga_mode: bool,
    pub manga_shift: bool,
    pub is_maximized: bool,
    pub window_mode: WindowMode,
    /// 全画面化の前にいたモード (Standard か Borderless)
    pub last_base_mode: WindowMode,
    pub effective_zoom: f32,
}

