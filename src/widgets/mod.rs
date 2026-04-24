pub mod menu;
pub mod toolbar;
pub mod sidebar;
pub mod dialogs;

pub use menu::*;
pub use toolbar::*;
pub use sidebar::*;
pub use dialogs::*;

/// アプリケーション全体で発生するアクション
#[derive(Clone, Debug, PartialEq)]
pub enum ViewerAction {
    About,
    ToggleLimiterMode,
    NextDir,
    PrevPage,
    NextPage,
    GoPrevDir,
    GoNextDir,
    Seek(usize),
    SetOpenFromEnd(bool), // config::DisplayMode は types::DisplayMode に移動したため修正
    SetDisplayMode(crate::types::DisplayMode),
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ToggleManga,
    ToggleMangaRtl,
    ToggleLinear,
    Rotate(bool),
    SetBgMode(crate::config::BackgroundMode),
    ToggleAlwaysOnTop,
    ToggleWindowResizable,
    ToggleWindowCentered, // config::DisplayMode は types::DisplayMode に移動したため修正
    OpenRecent(String),
    OpenFolder,
    RevealInExplorer,
    OpenExternal(usize),
    OpenExternalSettings,
    OpenKeyConfig,
    OpenSortSettings,
    ToggleMultipleInstances,
    ToggleDebug,
    SetMouseAction(usize, String),
    SetPdfRenderSize(u32),
    TogglePdfWarning,
    OpenLimiterSettings,
    SetLimiterPageDuration(f32),
    SetLimiterFolderDuration(f32),
    SetWindowMode(crate::types::WindowMode), // WindowMode 列挙型で一元管理
    ResizeWindow(u32, u32),
    MoveToCenter,
    Exit,
    ToggleTree,
}


/// アクションIDに対応する日本語ラベルを返す（キーコンフィグ画面等で使用）
pub fn get_action_label(id: &str) -> &str {
    match id {
        "PrevPage" => "前のページ",
        "NextPage" => "次のページ",
        "PrevPageSingle" => "前のページ (単一)",
        "NextPageSingle" => "次のページ (単一)",
        "PrevDir" => "前のフォルダ",
        "NextDir" => "次のフォルダ",
        "ToggleFit" => "フィット表示切替",
        "ToggleManga" => "マンガモード切替",
        "ToggleTree" => "ツリー表示切替",
        "ZoomIn" => "拡大",
        "ZoomOut" => "縮小",
        "RotateCW" => "右回転",
        "None" => "なし",
        _ => id,
    }
}