use eframe::egui;
use crate::config::Config;

/// プライマリモニターのワークエリア（タスクバーを除いた領域）を取得する
/// 戻り値: (x, y, width, height)
pub fn get_primary_work_area() -> (f32, f32, f32, f32) {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};
        use windows_sys::Win32::Foundation::RECT;
        let mut rect: RECT = std::mem::zeroed();
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut _ as _, 0) != 0 {
            return (
                rect.left as f32,
                rect.top as f32,
                (rect.right - rect.left) as f32,
                (rect.bottom - rect.top) as f32,
            );
        }
    }
    (0.0, 0.0, 1920.0, 1080.0)
}

/// ウィンドウを指定されたサイズに基づいてワークエリアの中央に配置する
pub fn move_to_center(ctx: &egui::Context, width: f32, height: f32) {
    let (wx, wy, ww, wh) = get_primary_work_area();
    let x = wx + (ww - width) / 2.0;
    let y = wy + (wh - height) / 2.0;
    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
}

/// 現在のウィンドウ状態（位置・サイズ・最大化）を解析して Config に反映する
pub fn sync_config_with_window(ctx: &egui::Context, config: &mut Config, last_resize_time: f64) {
    let now = ctx.input(|i| i.time);
    let viewport_info = ctx.input(|i| i.viewport().clone());
    
    let maximized = viewport_info.maximized.unwrap_or(false);
    let minimized = viewport_info.minimized.unwrap_or(false);
    let fullscreen = viewport_info.fullscreen.unwrap_or(false);

    // フルスクリーン時は「最大化状態」を上書き保存しない（解除後に戻すため）
    if !fullscreen {
        config.window_maximized = maximized;
    }

    // 通常状態（最大化・最小化・フルスクリーンではない）の時のみ、座標とサイズを記録する
    // 手動リサイズ直後（0.5秒間）は干渉防止のため記録をスキップ
    if !maximized && !minimized && !fullscreen && (last_resize_time == 0.0 || now - last_resize_time > 0.5) {
        if let Some(rect) = viewport_info.outer_rect {
            config.window_x = rect.min.x;
            config.window_y = rect.min.y;
        }
        if let Some(rect) = viewport_info.inner_rect {
            if rect.width() > 10.0 && rect.height() > 10.0 {
                config.window_width = rect.width();
                config.window_height = rect.height();
            }
        }
    }
}

/// 指定されたサイズへウィンドウをリサイズするコマンドを発行する
pub fn request_resize(ctx: &egui::Context, width: u32, height: u32) {
    let s = egui::vec2(width as f32, height as f32);
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(s));
}

/// H字型のアイコンデータを生成（ウィンドウ生成時に使用）
pub fn create_window_icon() -> egui::IconData {
    let size = 32usize;
    let mut rgba = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let i = (y * size + x) * 4;
            let is_h = (x >= 6 && x <= 10 && y >= 5 && y <= 26) ||
                       (x >= 21 && x <= 25 && y >= 5 && y <= 26) ||
                       (y >= 14 && y <= 17 && x > 10 && x < 21);
            if is_h { rgba[i..i+4].copy_from_slice(&[255, 255, 255, 255]); }
        }
    }
    egui::IconData { rgba, width: size as u32, height: size as u32 }
}