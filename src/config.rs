use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Debug)]
pub enum SortMode {
    Name,
    Mtime,
    Size,
}

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Debug)]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Config {
    /// 外部アプリのパス
    pub external_app: String,
    /// コマンドライン引数 (%P はパスに置換)
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            #[cfg(target_os = "windows")]
            external_app: "cmd".to_owned(),
            #[cfg(target_os = "windows")]
            external_args: vec![
                "/C".to_owned(),
                "start".to_owned(),
                "".to_owned(),
                "%P".to_owned(),
            ],
            #[cfg(not(target_os = "windows"))]
            external_app: "xdg-open".to_owned(),
            #[cfg(not(target_os = "windows"))]
            external_args: vec!["%P".to_owned()],
            linear_filter: true,
            allow_multiple_instances: false,
            sort_mode: SortMode::Name,
            sort_order: SortOrder::Ascending,
            sort_natural: true,
            manga_rtl: true,
        }
    }
}

/// 設定ファイルを読み込む。存在しない場合はデフォルトを書き出して返す。
pub fn load_config_file() -> (Config, Option<PathBuf>) {
    let config_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("config.ini")));

    let config = if let Some(ref path) = config_path {
        if let Ok(content) = std::fs::read_to_string(path) {
            toml::from_str::<Config>(&content).unwrap_or_default()
        } else {
            let default_cfg = Config::default();
            if let Ok(toml_str) = toml::to_string_pretty(&default_cfg) {
                let _ = std::fs::write(path, toml_str);
            }
            default_cfg
        }
    } else {
        Config::default()
    };

    (config, config_path)
}
