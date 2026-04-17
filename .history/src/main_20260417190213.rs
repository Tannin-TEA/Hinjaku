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
    let (config_name, path_arg) = integrator::parse_args(&std::env::args().collect::<Vec<_>>());

    // 2. 設定読み込み (INI対応)
    let (config, _config_path) = config::load_config_file(config_name.as_deref());

    // 3. 二重起動防止 (integrator への分離)
    let mut _mutex_handle = 0isize;
    if !config.allow_multiple_instances {
        if let Some(h) = integrator::check_single_instance() {
            _mutex_handle = h;
        } else {
            if let Some(path) = path_arg {
                integrator::send_path_to_existing_instance(&path);
            }
            return Ok(());
        }
    }

    // ウィンドウタイトルの決定 (デフォルト以外なら設定ファイル名を表示)
    let title = if let Some(ref name) = config_name {
        if name != "config.ini" {
            format!("Hinjaku - {}", name)
        } else {
            "Hinjaku".to_string()
        }
    } else {
        "Hinjaku".to_string()
    };

    // 4. UI起動設定
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_icon(std::sync::Arc::new(integrator::create_h_icon()))
            .with_inner_size([1024.0, 768.0])
            .with_drag_and_drop(true),
        // エラーの原因となっている wgpu_options を削除し、
        // より軽量な OpenGL (glow) レンダラーを指定します。
        renderer: eframe::Renderer::Glow,
        // 不要なバッファを無効化してメモリ消費を抑える
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        hardware_acceleration: eframe::HardwareAcceleration::Preferred,
        ..Default::default()
    };

    let initial_path = path_arg.and_then(|p| utils::clean_path(&p).into()); // utils::clean_path を使用
    let (ipc_tx, ipc_rx) = std::sync::mpsc::channel();

    // 単一インスタンスモードならサーバーを起動
    if !config.allow_multiple_instances {
        integrator::listen_for_opens(ipc_tx);
    }

    eframe::run_native(
        "Hinjaku",
        options,
        Box::new(move |cc| {
            let archive_reader = std::sync::Arc::new(archive::DefaultArchiveReader);
            Box::new(viewer::App::new(cc, initial_path, config_name, archive_reader, ipc_rx))
        }),
    )
}
