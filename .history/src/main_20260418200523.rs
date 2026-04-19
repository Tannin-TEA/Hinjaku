#![windows_subsystem = "windows"]

mod archive;
mod error;
mod utils; // utils モジュールをインポート
mod config;
mod constants;
mod integrator;
mod manager;
mod viewer;
mod painter;
mod widgets;
mod input;

fn main() -> eframe::Result<()> {
    // 1. 引数解析 (integrator への分離)
    let (config_name, path_arg, debug_cli, renderer_override) = integrator::parse_args(&std::env::args().collect::<Vec<_>>());

    if debug_cli {
        integrator::setup_console();
    }

    // 2. 設定読み込み (INI対応)
    let (mut config, _config_path) = config::load_config_file(config_name.as_deref());

    // コマンドライン引数によるレンダラーの強制上書き
    if let Some(r) = renderer_override {
        config.renderer = if r == "wgpu" { config::RendererMode::Wgpu } else { config::RendererMode::Glow };
    }

    // 3. 二重起動防止 (integrator への分離)
    let mut _mutex_handle = 0isize;
    if !config.allow_multiple_instances {
        if let Some(h) = integrator::check_single_instance() {
            _mutex_handle = h;
        } else {
            if let Some(path) = path_arg {
                integrator::send_path_via_wm_copydata(&path);
            }
            return Ok(());
        }
    }

    let renderer_str = match config.renderer {
        config::RendererMode::Glow => "OpenGL",
        config::RendererMode::Wgpu => "Wgpu",
    };

    // ウィンドウタイトルの決定 (デフォルト以外なら設定ファイル名を表示)
    let config_part = config_name.as_ref()
        .filter(|&n| n != "config.ini")
        .map(|n| format!(" {{{}}}", n))
        .unwrap_or_default();

    let title = format!("Hinjaku - {}{}", renderer_str, config_part);

    // 4. UI起動設定
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(title.clone())
        .with_icon(std::sync::Arc::new(integrator::create_h_icon()))
        .with_inner_size([config.window_width, config.window_height])
        .with_resizable(config.window_resizable)
        .with_drag_and_drop(true)
        .with_maximized(config.window_maximized);

    if config.window_centered {
        viewport = viewport.with_centered(true);
    } else {
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

    let initial_path = path_arg.and_then(|p| utils::clean_path(&p).into()); // utils::clean_path を使用

    eframe::run_native(
        "Hinjaku",
        options,
        Box::new({
            let title_clone = title.clone();
            move |cc| {
            let archive_reader = std::sync::Arc::new(archive::DefaultArchiveReader);
            Box::new(viewer::App::new(cc, initial_path, config_name, archive_reader, &title_clone, debug_cli))
        }}),
    )
}
