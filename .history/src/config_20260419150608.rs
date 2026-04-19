use crate::error::Result;
use ini::Ini;
use std::collections::HashMap;
use std::path::PathBuf;

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

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RendererMode {
    Glow,
    Wgpu,
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
    /// 外部アプリ設定 (最大5つ)
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
    /// レンダラーモード
    pub renderer: RendererMode,
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
    /// 試聴モード（ページ送り制限を有効にするか）
    pub preview_mode: bool,
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
            renderer: RendererMode::Glow,
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
            preview_mode: false,
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
                ("ToggleFullscreen", "Enter"),
                ("ToggleBorderless", "Alt+Enter"),
                ("Escape", "Escape"),
                ("ToggleTree", "T"),
                ("ToggleFit", "F"),
                ("ZoomIn", "Plus, Equals"),
                ("ZoomOut", "Minus"),
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
                ("ToggleLinear", "I"),
                ("ToggleMangaRtl", "Y"),
                ("Quit", "Q, Ctrl+W"),
                ("ToggleBg", "B"),
                ("ToggleDebug", "F12"),
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
            if let Some(v) = sec.get("PreviewMode") { cfg.preview_mode = v == "true"; }

            if let Some(v) = sec.get("Renderer") {
                cfg.renderer = match v.to_lowercase().as_str() {
                    "wgpu" => RendererMode::Wgpu,
                    _ => RendererMode::Glow,
                };
            }
        }

        for i in 0..5 {
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
        .set("Renderer", if cfg.renderer == RendererMode::Wgpu { "Wgpu" } else { "Glow" })
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
        .set("PreviewMode", cfg.preview_mode.to_string())
        .set("OpenFromEnd", cfg.open_from_end.to_string());
    
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
