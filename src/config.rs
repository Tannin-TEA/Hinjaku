use crate::error::Result;
use ini::Ini;
use std::collections::HashMap;
use std::path::PathBuf;
use crate::types::{DisplayMode, WindowMode};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SortMode {
    Name,
    Mtime,
    Size,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FilterMode {
    Nearest,
    Bilinear,
    Bicubic,
    Lanczos,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BackgroundMode {
    Theme,
    Black,
    Gray,
    White,
    Checkerboard,
    Green,
}

#[derive(Clone, Debug)]
pub struct ExternalAppConfig {
    pub name: String,
    pub exe: String,
    pub args: Vec<String>,
    pub close_after_launch: bool,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// 外部アプリ設定 (最大9つ)
    pub external_apps: Vec<ExternalAppConfig>,
    /// 画像の補正（スムージング）モード
    pub filter_mode: FilterMode,
    /// 複数起動を許可するか
    pub allow_multiple_instances: bool,
    /// 常に手前に表示するか
    pub always_on_top: bool,
    /// ソートモード
    pub sort_mode: SortMode,
    /// ソート順
    pub sort_order: SortOrder,
    /// 自然順ソートを有効にするか
    pub sort_natural: bool,
    /// 右開き (RTL) かどうか
    pub manga_rtl: bool,
    /// フォルダ移動時に末尾から開くか
    pub open_from_end: bool,
    /// 背景モード
    pub bg_mode: BackgroundMode,
    /// キーコンフィグ
    pub keys: HashMap<String, String>,
    /// 初回起動フラグ
    pub is_first_run: bool,
    /// ウィンドウの X 座標
    pub window_x: f32,
    /// ウィンドウの Y 座標
    pub window_y: f32,
    /// ウィンドウの幅
    pub window_width: f32,
    /// ウィンドウの高さ
    pub window_height: f32,
    /// 最近開いたパスの履歴 (最大10件)
    pub recent_paths: Vec<String>,
    /// ウィンドウのリサイズを許可するか
    pub window_resizable: bool,
    /// ウィンドウが最大化されているか
    pub window_maximized: bool,
    /// 起動時にウィンドウを画面中央に配置するか
    pub window_centered: bool,
    /// マウス第4ボタン（戻る）のアクション
    pub mouse4_action: String,
    /// マウス第5ボタン（進む）のアクション
    pub mouse5_action: String,
    /// マウス中ボタン（ホイールクリック）のアクション
    pub mouse_middle_action: String,
    /// リミッターモード（ページ送り制限を有効にするか）
    pub limiter_mode: bool,
    /// ページ送りリミッターの待機時間 (秒)
    pub limiter_page_duration: f32,
    /// フォルダ・アーカイブ移動リミッターの待機時間 (秒)
    pub limiter_folder_duration: f32,
    /// フォルダの最初で止まるか
    pub limiter_stop_at_start: bool,
    /// フォルダの最後で止まるか
    pub limiter_stop_at_end: bool,
    /// PDF警告を表示するか
    pub show_pdf_warning: bool,
    /// PDFのレンダリングDPI
    pub pdf_render_dpi: u32,
    /// マンガモード
    pub manga_mode: bool,
    /// 表示モード
    pub display_mode: DisplayMode,
    /// ウィンドウの表示形態 (標準/ボーダレス/フルスクリーン)
    pub window_mode: WindowMode,
    /// ツリーの表示
    pub show_tree: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            external_apps: vec![
                ExternalAppConfig {
                    name: "既定のプログラム".to_owned(),
                    exe: "cmd".to_owned(),
                    args: vec!["/C".to_owned(), "start".to_owned(), "".to_owned(), "%P".to_owned()],
                    close_after_launch: false,
                },
                ExternalAppConfig {
                    name: "例エキスプローラに送る".to_owned(),
                    exe: "explorer.exe".to_owned(),
                    args: vec!["/select,".to_owned(), "%P".to_owned()],
                    close_after_launch: false,
                },
                ExternalAppConfig { name: "未設定".to_owned(), exe: "".to_owned(), args: vec!["%P".to_owned()], close_after_launch: false },
                ExternalAppConfig { name: "未設定".to_owned(), exe: "".to_owned(), args: vec!["%P".to_owned()], close_after_launch: false },
                ExternalAppConfig { name: "未設定".to_owned(), exe: "".to_owned(), args: vec!["%P".to_owned()], close_after_launch: false },
                ExternalAppConfig { name: "未設定".to_owned(), exe: "".to_owned(), args: vec!["%P".to_owned()], close_after_launch: false },
                ExternalAppConfig { name: "未設定".to_owned(), exe: "".to_owned(), args: vec!["%P".to_owned()], close_after_launch: false },
                ExternalAppConfig { name: "未設定".to_owned(), exe: "".to_owned(), args: vec!["%P".to_owned()], close_after_launch: false },
                ExternalAppConfig { name: "未設定".to_owned(), exe: "".to_owned(), args: vec!["%P".to_owned()], close_after_launch: false },
            ],
            filter_mode: FilterMode::Bilinear,
            allow_multiple_instances: false,
            always_on_top: false,
            sort_mode: SortMode::Name,
            sort_order: SortOrder::Ascending,
            sort_natural: true,
            manga_rtl: true,
            open_from_end: false,
            bg_mode: BackgroundMode::Theme,
            is_first_run: true,
            window_x: 100.0,
            window_y: 100.0,
            window_width: 1024.0,
            window_height: 768.0,
            recent_paths: Vec::new(),
            window_resizable: true,
            window_maximized: false,
            window_centered: false,
            mouse4_action: "PrevPage".to_string(),
            mouse5_action: "NextPage".to_string(),
            mouse_middle_action: "ToggleFit".to_string(),
            limiter_mode: false,
            limiter_page_duration: 0.25,
            limiter_folder_duration: 0.5,
            limiter_stop_at_start: true,
            limiter_stop_at_end: false,
            show_pdf_warning: true,
            pdf_render_dpi: 96,
            manga_mode: false,
            display_mode: DisplayMode::Fit,
            window_mode: WindowMode::Standard,
            show_tree: false,
            keys: [
                ("PrevPage", "ArrowLeft, P"),
                ("NextPage", "ArrowRight, N"),
                ("Left", "ArrowLeft"),
                ("Right", "ArrowRight"),
                ("PrevPageSingle", "ArrowUp"),
                ("NextPageSingle", "ArrowDown"),
                ("Up", "ArrowUp"),
                ("Down", "ArrowDown"),
                ("Enter", "Enter"),
                ("ToggleMaximized", "Enter"),
                ("ToggleFullscreen", "Alt+Enter"),
                ("ToggleBorderless", "Shift+Enter"),
                ("Escape", "Escape"),
                ("ToggleTree", "T"),
                ("ToggleFit", "F"),
                ("ZoomIn", "Plus, Equals"),
                ("ZoomOut", "Minus"),
                ("ZoomReset", "Z"),
                ("ToggleManga", "M, Space"),
                ("RotateCW", "R"),
                ("OpenKeyConfig", "K"),
                ("RotateCCW", "Ctrl+R"),
                ("PrevDir", "PageUp"),
                ("NextDir", "PageDown"),
                ("SortSettings", "S"),
                ("FirstPage", "Home"),
                ("LastPage", "End"),
                ("RevealExplorer", "Backspace"),
                ("OpenExternal1", "E"),
                ("OpenExternal2", ""),
                ("OpenExternal3", ""),
                ("OpenExternal4", ""),
                ("OpenExternal5", ""),
                ("OpenExternal6", ""),
                ("OpenExternal7", ""),
                ("OpenExternal8", ""),
                ("OpenExternal9", ""),
                ("ToggleLinear", "I"),
                ("ToggleMangaRtl", "Y"),
                ("Quit", "Q, Ctrl+W"),
                ("ToggleBg", "B"),
                ("ToggleDebug", "F12"),
                ("ToggleLimiter", "L"),
                ("JumpPage", "J"),
            ].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }
}

pub fn load_config_file(custom_name: Option<&str>) -> (Config, Option<PathBuf>) {
    // ユーザー指定のINI名、またはデフォルトの config.ini を使用
    let filename = custom_name.unwrap_or("config.ini");
    let path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(filename)))
        .unwrap_or_else(|| PathBuf::from(filename));

    if let Ok(ini) = Ini::load_from_file(&path) {
        let mut cfg = Config::default();
        if let Some(sec) = ini.section(Some("Global") as Option<&str>) {
            if let Some(v) = sec.get("FilterMode") {
                cfg.filter_mode = match v.to_lowercase().as_str() {
                    "nearest" => FilterMode::Nearest,
                    "bicubic" => FilterMode::Bicubic,
                    "lanczos" => FilterMode::Lanczos,
                    _ => FilterMode::Bilinear,
                };
            } else if let Some(v) = sec.get("LinearFilter") { // 互換性のための旧設定読み込み
                cfg.filter_mode = if v == "true" { FilterMode::Bilinear } else { FilterMode::Nearest };
            }
            if let Some(v) = sec.get("AllowMultipleInstances") { cfg.allow_multiple_instances = v == "true"; }
            if let Some(v) = sec.get("AlwaysOnTop") { cfg.always_on_top = v == "true"; }
            if let Some(v) = sec.get("SortMode") {
                cfg.sort_mode = match v.to_lowercase().as_str() {
                    "mtime" => SortMode::Mtime,
                    "size"  => SortMode::Size,
                    _       => SortMode::Name,
                };
            }
            if let Some(v) = sec.get("SortOrder") {
                cfg.sort_order = match v.to_lowercase().as_str() {
                    "descending" => SortOrder::Descending,
                    _            => SortOrder::Ascending,
                };
            }
            if let Some(v) = sec.get("SortNatural") { cfg.sort_natural = v == "true"; }
            if let Some(v) = sec.get("MangaRtl") { cfg.manga_rtl = v == "true"; }
            if let Some(v) = sec.get("OpenFromEnd") { cfg.open_from_end = v == "true"; }
            if let Some(v) = sec.get("IsFirstRun") { cfg.is_first_run = v == "true"; }
            
            if let Some(v) = sec.get("BackgroundMode") {
                cfg.bg_mode = match v.to_lowercase().as_str() {
                    "black" => BackgroundMode::Black,
                    "gray" => BackgroundMode::Gray,
                    "white" => BackgroundMode::White,
                    "checkerboard" => BackgroundMode::Checkerboard,
                    "green" => BackgroundMode::Green,
                    _ => BackgroundMode::Theme,
                };
            }
            if let Some(v) = sec.get("WindowX") { if let Ok(f) = v.parse::<f32>() { cfg.window_x = f; } }
            if let Some(v) = sec.get("WindowY") { if let Ok(f) = v.parse::<f32>() { cfg.window_y = f; } }
            if let Some(v) = sec.get("WindowWidth") { if let Ok(f) = v.parse::<f32>() { cfg.window_width = f.max(100.0); } }
            if let Some(v) = sec.get("WindowHeight") { if let Ok(f) = v.parse::<f32>() { cfg.window_height = f.max(100.0); } }
            if let Some(v) = sec.get("RecentPaths") {
                cfg.recent_paths = v.split('|').map(|s| s.to_string()).filter(|s| !s.is_empty()).collect();
            }
            if let Some(v) = sec.get("WindowResizable") { cfg.window_resizable = v == "true"; }
            if let Some(v) = sec.get("WindowMaximized") { cfg.window_maximized = v == "true"; }
            if let Some(v) = sec.get("WindowCentered") { cfg.window_centered = v == "true"; }
            if let Some(v) = sec.get("Mouse4Action") { cfg.mouse4_action = v.to_string(); }
            if let Some(v) = sec.get("Mouse5Action") { cfg.mouse5_action = v.to_string(); }
            if let Some(v) = sec.get("MouseMiddleAction") { cfg.mouse_middle_action = v.to_string(); }

            // 互換性のため Preview/Reading キーもチェック
            if let Some(v) = sec.get("LimiterMode").or(sec.get("ReadingMode")).or(sec.get("PreviewMode")) {
                cfg.limiter_mode = v == "true";
            }
            if let Some(v) = sec.get("LimiterPageDuration").or(sec.get("LimiterDuration")).or(sec.get("ReadingGuardDuration")).or(sec.get("PreviewGuardDuration")) {
                if let Ok(f) = v.parse::<f32>() { cfg.limiter_page_duration = f; }
            }
            if let Some(v) = sec.get("LimiterFolderDuration") {
                if let Ok(f) = v.parse::<f32>() { cfg.limiter_folder_duration = f; }
            }
            if let Some(v) = sec.get("LimiterStopAtStart") { cfg.limiter_stop_at_start = v == "true"; }
            if let Some(v) = sec.get("LimiterStopAtEnd") { cfg.limiter_stop_at_end = v == "true"; }
            if let Some(v) = sec.get("PdfRenderDpi") { if let Ok(n) = v.parse::<u32>() { cfg.pdf_render_dpi = n.clamp(72, 600); } }

            if let Some(v) = sec.get("MangaMode") { cfg.manga_mode = v == "true"; }
            if let Some(v) = sec.get("DisplayMode") {
                cfg.display_mode = match v.to_lowercase().as_str() {
                    "windowfit" => DisplayMode::WindowFit,
                    "manual"    => DisplayMode::Manual,
                    _           => DisplayMode::Fit,
                };
            }
            if let Some(v) = sec.get("WindowMode") {
                cfg.window_mode = match v.to_lowercase().as_str() {
                    "borderless" => WindowMode::Borderless,
                    "fullscreen" => WindowMode::Fullscreen,
                    _            => WindowMode::Standard,
                };
            } else if sec.get("IsFullscreen") == Some("true") { cfg.window_mode = WindowMode::Fullscreen; }
            else if sec.get("IsSmallBorderless") == Some("true") { cfg.window_mode = WindowMode::Borderless; }

            if let Some(v) = sec.get("ShowTree") { cfg.show_tree = v == "true"; }
        }

        for i in 0..9 {
            let section_name = format!("App_{}", i + 1);
            if let Some(sec) = ini.section(Some(&section_name)) {
                if let Some(v) = sec.get("Name") { cfg.external_apps[i].name = v.to_string(); }
                if let Some(v) = sec.get("ExecutePath") { cfg.external_apps[i].exe = v.to_string(); }
                if let Some(v) = sec.get("Args") {
                    cfg.external_apps[i].args = v.split_whitespace().map(|s| s.to_string()).collect();
                }
                if let Some(v) = sec.get("CloseAfterLaunch") { cfg.external_apps[i].close_after_launch = v == "true"; }
            }
        }

        if let Some(sec) = ini.section(Some("KeyConfig")) {
            for (k, v) in sec.iter() {
                cfg.keys.insert(k.to_string(), v.to_string());
            }
        }

        // キーコンフィグが必要な場合、ここに [KeyConfig] セクションの読み込みを追加
        (cfg, Some(path))
    } else {
        let mut cfg = Config::default();
        // コマンドラインでINIファイルが指定された場合は、初回起動フラグを立てない
        if custom_name.is_some() {
            cfg.is_first_run = false;
        }
        let _ = save_config_file(&cfg, &path);
        (cfg, Some(path))
    }
}

pub fn save_config_file(cfg: &Config, path: &std::path::Path) -> Result<()> {
    let mut ini = Ini::new();
    ini.with_section(Some("Global"))
        .set("FilterMode", match cfg.filter_mode {
            FilterMode::Nearest => "Nearest",
            FilterMode::Bilinear => "Bilinear",
            FilterMode::Bicubic => "Bicubic",
            FilterMode::Lanczos => "Lanczos",
        })
        .set("AllowMultipleInstances", cfg.allow_multiple_instances.to_string())
        .set("AlwaysOnTop", cfg.always_on_top.to_string())
        .set("SortMode", match cfg.sort_mode {
            SortMode::Name  => "Name",
            SortMode::Mtime => "Mtime",
            SortMode::Size  => "Size",
        })
        .set("SortOrder", match cfg.sort_order {
            SortOrder::Ascending  => "Ascending",
            SortOrder::Descending => "Descending",
        })
        .set("SortNatural", cfg.sort_natural.to_string())
        .set("MangaRtl", cfg.manga_rtl.to_string())
        .set("IsFirstRun", cfg.is_first_run.to_string())
        .set("WindowX", cfg.window_x.to_string())
        .set("WindowY", cfg.window_y.to_string())
        .set("WindowWidth", cfg.window_width.to_string())
        .set("WindowHeight", cfg.window_height.to_string())
        .set("WindowResizable", cfg.window_resizable.to_string())
        .set("WindowMaximized", cfg.window_maximized.to_string())
        .set("WindowCentered", cfg.window_centered.to_string())
        .set("RecentPaths", cfg.recent_paths.join("|"))
        .set("Mouse4Action", &cfg.mouse4_action)
        .set("Mouse5Action", &cfg.mouse5_action)
        .set("MouseMiddleAction", &cfg.mouse_middle_action)
        .set("LimiterMode", cfg.limiter_mode.to_string())
        .set("LimiterPageDuration", cfg.limiter_page_duration.to_string())
        .set("LimiterFolderDuration", cfg.limiter_folder_duration.to_string())
        .set("LimiterStopAtStart", cfg.limiter_stop_at_start.to_string())
        .set("LimiterStopAtEnd", cfg.limiter_stop_at_end.to_string())
        .set("ShowPdfWarning", cfg.show_pdf_warning.to_string())
        .set("PdfRenderDpi", cfg.pdf_render_dpi.to_string())
        .set("OpenFromEnd", cfg.open_from_end.to_string())
        .set("MangaMode", cfg.manga_mode.to_string())
        .set("DisplayMode", match cfg.display_mode {
            DisplayMode::Fit => "Fit",
            DisplayMode::WindowFit => "WindowFit",
            DisplayMode::Manual => "Manual",
        })
        .set("WindowMode", match cfg.window_mode {
            WindowMode::Standard => "Standard",
            WindowMode::Borderless => "Borderless",
            WindowMode::Fullscreen => "Fullscreen",
        })
        .set("ShowTree", cfg.show_tree.to_string());
    
    for (i, app) in cfg.external_apps.iter().enumerate() {
        ini.with_section(Some(format!("App_{}", i + 1)))
            .set("Name", &app.name)
            .set("ExecutePath", &app.exe)
            .set("Args", app.args.join(" "))
            .set("CloseAfterLaunch", app.close_after_launch.to_string());
    }

    let mut key_sec = ini.with_section(Some("KeyConfig"));
    for (k, v) in &cfg.keys {
        key_sec.set(k, v);
    }

    // 将来的に KeyConfig セクションもここに保存
    Ok(ini.write_to_file(path)?)
}
