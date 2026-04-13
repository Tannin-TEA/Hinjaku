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
    pub external_app: String,
    pub external_args: Vec<String>,
    pub linear_filter: bool,
    pub allow_multiple_instances: bool,
    pub sort_mode: SortMode,
    pub sort_order: SortOrder,
    pub sort_natural: bool,
    pub manga_rtl: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            #[cfg(target_os = "windows")]
            external_app: "cmd".to_owned(),
            #[cfg(target_os = "windows")]
            external_args: vec!["/C".to_owned(), "start".to_owned(), "".to_owned(), "%P".to_owned()],
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
                let _ = std::fs::write(path, tml_str);
            }
            default_cfg
        }
    } else {
        Config::default()
    };

    (config, config_path)
}