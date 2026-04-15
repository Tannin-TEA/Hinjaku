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
    /// 外部アプリ設定 (最大5つ)
    pub external_apps: Vec<ExternalAppConfig>,
    /// 画像の補正（スムージング）を有効にするか
    pub linear_filter: bool,
    /// 複数起動を許可するか
    pub allow_multiple_instances: bool,
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
    /// 外部アプリ設定を保存した後にウィンドウを閉じるか
    pub close_settings_on_apply: bool,
    /// キーコンフィグ
    pub keys: HashMap<String, String>,
    /// 初回起動フラグ
    pub is_first_run: bool,
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
            linear_filter: true,
            allow_multiple_instances: false,
            sort_mode: SortMode::Name,
            sort_order: SortOrder::Ascending,
            sort_natural: true,
            manga_rtl: true,
            open_from_end: false,
            bg_mode: BackgroundMode::Theme,
            close_settings_on_apply: true,
            is_first_run: true,
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
            if let Some(v) = sec.get("LinearFilter") { cfg.linear_filter = v == "true"; }
            if let Some(v) = sec.get("AllowMultipleInstances") { cfg.allow_multiple_instances = v == "true"; }
            if let Some(v) = sec.get("SortNatural") { cfg.sort_natural = v == "true"; }
            if let Some(v) = sec.get("MangaRtl") { cfg.manga_rtl = v == "true"; }
            if let Some(v) = sec.get("OpenFromEnd") { cfg.open_from_end = v == "true"; }
            if let Some(v) = sec.get("CloseSettingsOnApply") { cfg.close_settings_on_apply = v == "true"; }
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
        .set("LinearFilter", cfg.linear_filter.to_string())
        .set("AllowMultipleInstances", cfg.allow_multiple_instances.to_string())
        .set("SortNatural", cfg.sort_natural.to_string())
        .set("MangaRtl", cfg.manga_rtl.to_string())
        .set("CloseSettingsOnApply", cfg.close_settings_on_apply.to_string())
        .set("IsFirstRun", cfg.is_first_run.to_string());
    ini.with_section(Some("Global")).set("OpenFromEnd", cfg.open_from_end.to_string());
    
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
