use eframe::egui;
use crate::config::Config;

/// ウィンドウをワークエリア（タスクバー除外）の中央に配置する
/// Windows上は純粋なWindows APIで完結させ、座標系の不一致を回避する
pub fn move_to_center(_ctx: &egui::Context, _inner_width: f32, _inner_height: f32) -> bool {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SystemParametersInfoW, SPI_GETWORKAREA,
            EnumWindows, GetWindowThreadProcessId,
            GetWindowRect, SetWindowPos, SWP_NOSIZE, SWP_NOZORDER,
        };
        use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
        use windows_sys::Win32::System::Threading::GetCurrentProcessId;

        // 自プロセスのトップレベルウィンドウを列挙して取得
        // IsWindowVisible は起動直後falseになることがあるため、サイズで判定する
        struct Search { hwnd: HWND, pid: u32 }
        unsafe extern "system" fn find_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
            let s = &mut *(lparam as *mut Search);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == s.pid {
                let mut r: RECT = std::mem::zeroed();
                if GetWindowRect(hwnd, &mut r) != 0 && (r.right - r.left) > 10 && (r.bottom - r.top) > 10 {
                    s.hwnd = hwnd;
                    return 0; // 発見、列挙終了
                }
            }
            1 // 続行
        }

        let mut search = Search { hwnd: std::ptr::null_mut(), pid: GetCurrentProcessId() };
        EnumWindows(Some(find_proc), &mut search as *mut _ as LPARAM);
        if search.hwnd.is_null() { return false; }

        // 実際のウィンドウ外寸を取得（タイトルバー・枠込み）
        let mut win_rect: RECT = std::mem::zeroed();
        if GetWindowRect(search.hwnd, &mut win_rect) == 0 { return false; }
        let win_w = win_rect.right  - win_rect.left;
        let win_h = win_rect.bottom - win_rect.top;

        // タスクバーを除いたワークエリアを取得
        let mut work: RECT = std::mem::zeroed();
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut _ as _, 0) == 0 { return false; }
        let work_w = work.right  - work.left;
        let work_h = work.bottom - work.top;

        let x = work.left + (work_w - win_w) / 2;
        let y = work.top  + (work_h - win_h) / 2;

        // すべて同一のWindows座標系で計算しているため変換不要
        SetWindowPos(search.hwnd, std::ptr::null_mut(), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
        true
    }
    #[cfg(not(target_os = "windows"))]
    {
        let (wx, wy, ww, wh) = get_primary_work_area();
        let (ow, oh) = ctx.input(|i| i.viewport().outer_rect)
            .map(|r| (r.width(), r.height()))
            .unwrap_or((inner_width, inner_height));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
            wx + (ww - ow) / 2.0,
            wy + (wh - oh) / 2.0,
        )));
        return true;
    }
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
    // 中央配置がONの場合は位置を保存しない（次回起動時に再度中央配置するため）
    if !maximized && !minimized && !fullscreen && (last_resize_time == 0.0 || now - last_resize_time > 0.5) {
        if !config.window_centered {
            if let Some(rect) = viewport_info.outer_rect {
                config.window_x = rect.min.x;
                config.window_y = rect.min.y;
            }
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
            let is_h = ((6..=10).contains(&x) && (5..=26).contains(&y)) ||
                       ((21..=25).contains(&x) && (5..=26).contains(&y)) ||
                       ((14..=17).contains(&y) && x > 10 && x < 21);
            if is_h { rgba[i..i+4].copy_from_slice(&[255, 255, 255, 255]); }
        }
    }
    egui::IconData { rgba, width: size as u32, height: size as u32 }
}