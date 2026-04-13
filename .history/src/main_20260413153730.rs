#![windows_subsystem = "windows"]

use std::io::Write;
use std::net::TcpStream;

mod archive;
mod viewer;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ArchView")
            .with_inner_size([1024.0, 768.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    // 設定の読み込み
    let (config, _) = viewer::load_config_file();

    let args: Vec<String> = std::env::args().collect();
    let initial_path = args.get(1).map(std::path::PathBuf::from);

    // 単一インスタンス起動とパス転送の制御
    let mut listener = None;
    if !config.allow_multiple_instances {
        const PORT: u16 = 43210;
        match std::net::TcpListener::bind(("127.0.0.1", PORT)) {
            Ok(l) => {
                listener = Some(l);
            }
            Err(_) => {
                // 既に起動している場合、引数のパスを送って終了
                if let Some(path) = initial_path {
                    if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", PORT)) {
                        let _ = stream.write_all(path.to_string_lossy().as_bytes());
                    }
                }
                return Ok(());
            }
        }
    }

    eframe::run_native(
        "ArchView",
        options,
        Box::new(move |cc| Box::new(viewer::App::new(cc, initial_path, listener))),
    )
}
