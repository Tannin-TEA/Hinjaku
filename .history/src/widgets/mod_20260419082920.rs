mod dialogs;
mod menu;
mod sidebar;
mod toolbar;

pub use dialogs::{about_window, debug_window, key_config_window, settings_window, sort_settings_window};
pub use menu::main_menu_bar;
pub use sidebar::sidebar_ui;
pub use toolbar::bottom_toolbar;

use crate::config;
use crate::types::DisplayMode;

/// ユーザーがUI操作を通じて要求したアクション
pub enum ViewerAction {
    OpenRecent(String),
    OpenFolder,
    RevealInExplorer,
    OpenExternal(usize),
    OpenExternalSettings,
    OpenKeyConfig,
    Exit,
    SetDisplayMode(DisplayMode),
    ZoomIn,
    ZoomOut,
    ToggleManga,
    ToggleMangaRtl,
    ToggleTree,
    OpenSortSettings,
    ToggleAlwaysOnTop,
    ToggleMultipleInstances,
    ToggleLinear,
    Rotate(bool), // true = CW, false = CCW
    GoPrevDir,
    GoNextDir,
    SetOpenFromEnd(bool),
    SetBgMode(config::BackgroundMode),
    PrevPage,
    NextPage,
    NextDir,
    Seek(usize),
    ToggleDebug,
    SetRenderer(config::RendererMode),
    ToggleWindowResizable,
    MoveToCenter,
    ToggleWindowCentered,
    ResizeWindow(u32, u32),
    About,
    SetMouseAction(u8, String),
}

/// キーコンフィグ画面やマウスボタン設定で表示するアクション名の日本語訳
pub(crate) fn get_action_label(id: &str) -> &str {
    match id {
        "PrevPage"         => "前のページを表示",
        "NextPage"         => "次のページを表示",
        "PrevPageSingle"   => "前のページを表示 (1枚送り)",
        "NextPageSingle"   => "次のページを表示 (1枚送り)",
        "Left"             => "左 (移動/ツリー操作)",
        "Right"            => "右 (移動/ツリー操作)",
        "Up"               => "上 (移動/ツリー操作)",
        "Down"             => "下 (移動/ツリー操作)",
        "Enter"            => "決定 (ツリー選択/ダイアログ)",
        "OpenKeyConfig"    => "キーコンフィグ画面を開く",
        "ToggleFullscreen" => "全画面表示の切替",
        "ToggleBorderless" => "ボーダレス全画面の切替",
        "Escape"           => "閉じる/解除/終了",
        "ToggleTree"       => "ディレクトリツリーの表示切替",
        "ToggleFit"        => "画像フィットモードの切替",
        "ZoomIn"           => "拡大",
        "ZoomOut"          => "縮小",
        "ToggleManga"      => "マンガモード(見開き)の切替",
        "RotateCW"         => "画像を右に回転",
        "RotateCCW"        => "画像を左に回転",
        "PrevDir"          => "前のフォルダ/アーカイブへ",
        "NextDir"          => "次のフォルダ/アーカイブへ",
        "SortSettings"     => "ソート設定ウィンドウを開く",
        "FirstPage"        => "最初のページへ移動",
        "LastPage"         => "最後のページへ移動",
        "RevealExplorer"   => "エクスプローラーで表示",
        "OpenExternal1"    => "外部アプリ1で開く",
        "OpenExternal2"    => "外部アプリ2で開く",
        "OpenExternal3"    => "外部アプリ3で開く",
        "OpenExternal4"    => "外部アプリ4で開く",
        "OpenExternal5"    => "外部アプリ5で開く",
        "ToggleLinear"     => "画像補正(スムージング)の切替",
        "ToggleMangaRtl"   => "右開き/左開きの切替",
        "Quit"             => "アプリを終了",
        "ToggleBg"         => "背景色の切替",
        "ToggleDebug"      => "デバッグ情報の表示切替",
        "None"             => "（なし）",
        _                  => id,
    }
}
