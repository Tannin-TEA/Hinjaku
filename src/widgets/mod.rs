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
#[allow(dead_code)]
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
    WindSizeLock,
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
        // ページ移動
        "PrevPage"       => "前のページ",
        "NextPage"       => "次のページ",
        "PrevPageSingle" => "前のページ（単頁）",
        "NextPageSingle" => "次のページ（単頁）",
        "FirstPage"      => "最初のページ",
        "LastPage"       => "最後のページ",
        "JumpPage"       => "ページジャンプ",
        "PrevDir"        => "前のフォルダ",
        "NextDir"        => "次のフォルダ",
        // 画像操作
        "ToggleFit"      => "表示フィット切替",
        "ZoomIn"         => "拡大",
        "ZoomOut"        => "縮小",
        "ZoomReset"      => "倍率リセット",
        "ToggleManga"    => "見開きモード切替",
        "ToggleMangaRtl" => "右←左 / 左→右 切替",
        "RotateCW"       => "右回転",
        "RotateCCW"      => "左回転",
        "ToggleLinear"   => "フィルター切替",
        "ToggleBg"       => "背景色切替",
        // ツリー・フォルダ操作
        "Up"             => "上",
        "Down"           => "下",
        "Left"           => "左",
        "Right"          => "右",
        "Enter"          => "決定",
        "ToggleTree"     => "ツリー表示切替",
        "RevealExplorer" => "エクスプローラーで表示",
        // システム・その他
        "ToggleMaximized"       => "最大化切替",
        "ToggleFullscreen"      => "全画面切替",
        "ToggleBorderless"      => "ボーダレス切替",
        "ToggleSmallBorderless" => "小型ボーダレス切替",
        "Escape"         => "Esc / 閉じる",
        "SortSettings"   => "ソート設定",
        "OpenKeyConfig"  => "キーコンフィグ設定",
        "ToggleDebug"    => "デバッグ表示",
        "Quit"           => "終了",
        "OpenExternal1"  => "外部アプリ 1 で開く",
        "OpenExternal2"  => "外部アプリ 2 で開く",
        "OpenExternal3"  => "外部アプリ 3 で開く",
        "OpenExternal4"  => "外部アプリ 4 で開く",
        "OpenExternal5"  => "外部アプリ 5 で開く",
        "OpenExternal6"  => "外部アプリ 6 で開く",
        "OpenExternal7"  => "外部アプリ 7 で開く",
        "OpenExternal8"  => "外部アプリ 8 で開く",
        "OpenExternal9"  => "外部アプリ 9 で開く",
        // マウス
        "WindSizeLock"   => "ウィンドウサイズ固定",
        "None"           => "なし",
        _ => id,
    }
}