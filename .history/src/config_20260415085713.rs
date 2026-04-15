use ini::Ini;
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
}

#[derive(Clone, Debug)]
pub struct Config {
    pub external_app: String,
    pub external_args: Vec<String>,
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
    /// 背景モード
    pub bg_mode: BackgroundMode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            external_app: "cmd".to_owned(),
            external_args: vec![
                "/C".to_owned(),
                "start".to_owned(),
                "".to_owned(),
                "%P".to_owned(),
            ],
            linear_filter: true,
            allow_multiple_instances: false,
            sort_mode: SortMode::Name,
            sort_order: SortOrder::Ascending,
            sort_natural: true,
            manga_rtl: true,
            bg_mode: BackgroundMode::Theme,
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
            // BackgroundMode, SortMode 等のパースロジック（省略可、デフォルト維持）
        }
        if let Some(sec) = ini.section(Some("App_Default") as Option<&str>) {
            if let Some(v) = sec.get("ExecutePath") { cfg.external_app = v.to_string(); }
            if let Some(v) = sec.get("Args") {
                cfg.external_args = v.split_whitespace().map(|s: &str| s.to_string()).collect();
            }
        }
        // キーコンフィグが必要な場合、ここに [KeyConfig] セクションの読み込みを追加
        (cfg, Some(path))
    } else {
        let cfg = Config::default();
        save_config_file(&cfg, &path);
        (cfg, Some(path))
    }
}

pub fn save_config_file(cfg: &Config, path: &std::path::Path) {
    let mut ini = Ini::new();
    ini.with_section(Some("Global"))
        .set("LinearFilter", cfg.linear_filter.to_string())
        .set("AllowMultipleInstances", cfg.allow_multiple_instances.to_string())
        .set("SortNatural", cfg.sort_natural.to_string())
        .set("MangaRtl", cfg.manga_rtl.to_string());
    
    ini.with_section(Some("App_Default"))
        .set("ExecutePath", &cfg.external_app)
        .set("Args", cfg.external_args.join(" "));

    // 将来的に KeyConfig セクションもここに保存
    let _ = ini.write_to_file(path);
}
