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
    let args: Vec<String> = std::env::args().collect();
    let (config_name, mut path_arg, debug_cli, pro_mode) = startup::parse_args(&args);

    // 他のプロセスへ渡す際や起動時に不整合が起きないよう、パスを絶対パスに変換
    if let Some(p) = path_arg.as_ref() {
        if let Ok(abs) = std::fs::canonicalize(p) {
            path_arg = Some(abs);
        }
    }

    // 1. Mutexチェックと二重起動時の自決 (GUI初期化前)
    let (config, config_path) = config::load_config_file(config_name.as_deref());
    let mut _mutex_handle = 0isize;
    if !config.allow_multiple_instances {
        if let Some(h) = startup::check_single_instance() {
            _mutex_handle = h;
        } else {
            // プロセスAが見つかれば引数を投げて終了
            if let Some(path) = path_arg {
                integrator::send_path_to_existing_instance(&path);
            }
            std::process::exit(0);
        }
    }

    if debug_cli { startup::setup_console(); }

    let title = startup::build_window_title(config_name.as_deref(), pro_mode, None);

    // 4. UI起動設定
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(title.clone())
        .with_icon(std::sync::Arc::new(window::create_window_icon()))
        .with_inner_size([config.window_width, config.window_height])
        .with_resizable(config.window_resizable)
        .with_drag_and_drop(true)
        .with_maximized(config.window_maximized)
        // 指示されたモードに応じて枠の有無と全画面状態を初期化
        .with_decorations(match config.window_mode {
            crate::types::WindowMode::Standard => true,
            crate::types::WindowMode::Borderless | crate::types::WindowMode::Fullscreen => false,
        })
        .with_fullscreen(match config.window_mode {
            crate::types::WindowMode::Fullscreen => true,
            _ => false,
        });

    // 「中央に配置」がオフの場合のみ、保存された座標を適用する
    if !config.window_centered {
        viewport = viewport.with_position([config.window_x, config.window_y]);
    }

    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        // 不要なバッファを無効化してメモリ消費を抑える
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        // WGPUを使用する場合、ハードウェア加速を優先
        hardware_acceleration: eframe::HardwareAcceleration::Preferred,
        ..Default::default()
    };

    let initial_path = path_arg.map(|p| utils::clean_path(&p));

    eframe::run_native(
        "Hinjaku",
        options,
        Box::new(
            move |cc| {
            let archive_reader = std::sync::Arc::new(archive::DefaultArchiveReader);
            Box::new(viewer::App::new(cc, initial_path, config, config_path, archive_reader, debug_cli, pro_mode))
        }),
    )
}
