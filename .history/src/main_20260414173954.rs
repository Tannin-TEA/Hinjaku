#![windows_subsystem = "windows"]

use std::sync::Arc;

mod archive;
mod config;
mod manager;
mod viewer;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut config_name = None;
    let mut path_arg = None;

    let mut i = 1;
    while i < args.len() {
        if (args[i] == "-c" || args[i] == "--config") && i + 1 < args.len() {
            config_name = Some(args[i + 1].clone());
            i += 2;
        } else if path_arg.is_none() && !args[i].starts_with('-') {
            path_arg = Some(std::path::PathBuf::from(&args[i]));
            i += 1;
        } else {
            i += 1;
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Hinjaku")
            .with_icon(Arc::new(create_h_icon()))
            .with_inner_size([1024.0, 768.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };

    let (config, _) = config::load_config_file(config_name.as_deref());

    // 他のアプリ（エクスプローラーの「送る」やファイラーの連携）からの相対パスを
    // 確実に処理するため、絶対パスに変換してから受け渡し（IPC）を行います。
    let initial_path = path_arg.map(|p| {
        if p.is_relative() {
            std::env::current_dir().ok().map(|cwd| cwd.join(&p)).unwrap_or(p)
        } else {
            p
        }
    });

    // 二重起動防止用の Mutex ハンドルを保持する変数
    let mut _mutex_handle = 0isize;

    if !config.allow_multiple_instances {
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
            use windows_sys::Win32::System::Threading::CreateMutexW;

            // アプリ固有の一意な名前（Local\ プレフィックスは現在のセッション内のみ）
            let name: Vec<u16> = "Local\\Hinjaku-Unique-Mutex-Name\0".encode_utf16().collect();
            unsafe {
                let handle = CreateMutexW(std::ptr::null(), 1, name.as_ptr());
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    // 既に Mutex が存在するため、終了する
                    return Ok(());
                }
                _mutex_handle = handle; // ハンドルをドロップさせないように保持
            }
        }
    }

    eframe::run_native(
        "Hinjaku",
        options,
        Box::new(move |cc| Box::new(viewer::App::new(cc, initial_path, config_name))),
    )
}

/// アプリアイコン（H 字）を生成する
fn create_h_icon() -> egui::IconData {
    let size = 32usize;
    let mut rgba = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let i = (y * size + x) * 4;
            let is_left_bar = x >= 6 && x <= 10 && y >= 5 && y <= 26;
            let is_right_bar = x >= 21 && x <= 25 && y >= 5 && y <= 26;
            let is_mid_bar = y >= 14 && y <= 17 && x > 10 && x < 21;
            if is_left_bar || is_right_bar || is_mid_bar {
                rgba[i] = 255;
                rgba[i + 1] = 255;
                rgba[i + 2] = 255;
                rgba[i + 3] = 255;
            }
            // else: rgba[i+3] はすでに 0（透明）
        }
    }
    egui::IconData {
        rgba,
        width: size as u32,
        height: size as u32,
    }
}
