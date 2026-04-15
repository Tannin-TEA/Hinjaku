#![windows_subsystem = "windows"]

mod archive;
mod config;
mod integrator;
mod manager;
mod viewer;

fn main() -> eframe::Result<()> {
    // 1. 引数解析 (integrator への分離)
    let (config_name, path_arg) = integrator::parse_args(&std::env::args().collect::<Vec<_>>());

    // 2. 設定読み込み (INI対応)
    let (config, config_path) = config::load_config_file(config_name.as_deref());

    // 3. 二重起動防止 (integrator への分離)
    let mut _mutex_handle = 0isize;
    if !config.allow_multiple_instances {
        if let Some(h) = integrator::check_single_instance() {
            _mutex_handle = h;
        } else { return Ok(()); }
    }

    // 4. UI起動設定
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Hinjaku")
            .with_icon(std::sync::Arc::new(integrator::create_h_icon()))
            .with_inner_size([1024.0, 768.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    let initial_path = path_arg.and_then(|p| archive::clean_path(&p).into());

    eframe::run_native(
        "Hinjaku",
        options,
        Box::new(move |cc| Box::new(viewer::App::new(cc, initial_path, config_path.map(|p| p.to_string_lossy().into_owned())))),
    )
}
