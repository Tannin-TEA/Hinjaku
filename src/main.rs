#![windows_subsystem = "windows"]

mod archive;
mod error;
mod types;
mod utils;
mod config;
mod constants;
mod nav_tree;
mod integrator;
mod manager;
mod viewer;
mod painter;
mod window;
mod shell;
mod startup;
mod pdf_handler;
mod widgets;
mod input;
mod toast;
#[cfg(target_os = "windows")]
mod wic;

fn main() -> eframe::Result<()> {
    // 1. 引数解析
    let (config_name, path_arg, debug_cli, renderer_override, pro_mode) = startup::parse_args(&std::env::args().collect::<Vec<_>>());

    if debug_cli {
        startup::setup_console();
    }

    // 2. 設定読み込み (INI対応)
    let (mut config, config_path) = config::load_config_file(config_name.as_deref());

    // コマンドライン引数によるレンダラーの強制上書き
    if let Some(r) = renderer_override {
        config.renderer = if r == "wgpu" { config::RendererMode::Wgpu } else { config::RendererMode::Glow };
    }

    // 3. 二重起動防止 (integrator への分離)
    let mut _mutex_handle = 0isize;
    if !config.allow_multiple_instances {
        if let Some(h) = startup::check_single_instance() {
            _mutex_handle = h;
        } else {
            if let Some(path) = path_arg {
                integrator::send_path_via_wm_copydata(&path);
            }
            return Ok(());
        }
    }

    let title = startup::build_window_title(config_name.as_deref(), &config.renderer, pro_mode);

    // 4. UI起動設定
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(title.clone())
        .with_icon(std::sync::Arc::new(window::create_window_icon()))
        .with_inner_size([config.window_width, config.window_height])
        .with_resizable(config.window_resizable)
        .with_drag_and_drop(true)
        .with_maximized(config.window_maximized);

    // 「中央に配置」がオフの場合のみ、保存された座標を適用する
    if !config.window_centered {
        viewport = viewport.with_position([config.window_x, config.window_y]);
    }

    let options = eframe::NativeOptions {
        viewport,
        // 設定に基づいてレンダラーを切り替える
        renderer: if config.renderer == config::RendererMode::Wgpu {
            eframe::Renderer::Wgpu
        } else {
            eframe::Renderer::Glow
        },
        // 不要なバッファを無効化してメモリ消費を抑える
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        hardware_acceleration: eframe::HardwareAcceleration::Preferred,
        ..Default::default()
    };

    let initial_path = path_arg.map(|p| utils::clean_path(&p));

    eframe::run_native(
        "Hinjaku",
        options,
        Box::new({
            let title_clone = title.clone();
            move |cc| {
            let archive_reader = std::sync::Arc::new(archive::DefaultArchiveReader);
            Box::new(viewer::App::new(cc, initial_path, config, config_path, archive_reader, &title_clone, debug_cli, pro_mode))
        }}),
    )
}
