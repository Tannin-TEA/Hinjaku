#![windows_subsystem = "windows"]

use std::io::Write;
use std::sync::Arc;
use std::net::TcpStream;

mod archive;
mod viewer;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Hinjaku")
            .with_icon(Arc::new(create_h_icon()))
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
        "Hinjaku",
        options,
        Box::new(move |cc| Box::new(viewer::App::new(cc, initial_path, listener))),
    )
}

/// アプリのアイコン（H）を生成する
fn create_h_icon() -> egui::IconData {
    let size = 32;
    let mut rgba = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let i = (y * size + x) * 4;
            
            // 32x32 の中で「H」の形を描く判定
            // 左右の縦棒
            let is_left_bar  = x >= 6 && x <= 10 && y >= 5 && y <= 26;
            let is_right_bar = x >= 21 && x <= 25 && y >= 5 && y <= 26;
            // 真ん中の横棒
            let is_mid_bar   = y >= 14 && y <= 17 && x > 10 && x < 21;

            if is_left_bar || is_right_bar || is_mid_bar {
                rgba[i]     = 255; // R
                rgba[i + 1] = 255; // G
                rgba[i + 2] = 255; // B
                rgba[i + 3] = 255; // A (不透明)
            } else {
                rgba[i + 3] = 0;   // A (透明)
            }
        }
    }
    egui::IconData { rgba, width: size as u32, height: size as u32 }
}
