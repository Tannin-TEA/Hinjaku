use std::path::PathBuf;

/// コマンドライン引数を解析して (INI名, 対象パス, デバッグフラグ, レンダラー上書き, proモード) を返す
pub fn parse_args(args: &[String]) -> (Option<String>, Option<PathBuf>, bool, Option<String>, bool) {
    let mut config_name = None;
    let mut path_arg = None;
    let mut debug_mode = false;
    let mut renderer_override = None;
    let mut pro_mode = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-c" | "--config" => {
                if i + 1 < args.len() {
                    config_name = Some(args[i + 1].clone());
                    i += 2;
                } else { i += 1; }
            }
            "-d" | "--debug" => {
                debug_mode = true;
                i += 1;
            }
            "-W" => {
                renderer_override = Some("wgpu".to_string());
                i += 1;
            }
            "-O" => {
                renderer_override = Some("glow".to_string());
                i += 1;
            }
            "-pro" => {
                pro_mode = true;
                i += 1;
            }
            _ if !args[i].starts_with('-') && path_arg.is_none() => {
                path_arg = Some(PathBuf::from(&args[i]));
                i += 1;
            }
            _ => i += 1,
        }
    }
    (config_name, path_arg, debug_mode, renderer_override, pro_mode)
}

/// Windows環境での二重起動防止チェック
pub fn check_single_instance() -> Option<isize> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
        use windows_sys::Win32::System::Threading::CreateMutexW;
        let name: Vec<u16> = "Local\\Hinjaku-Unique-Mutex-Name\0".encode_utf16().collect();
        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 1, name.as_ptr());
            if GetLastError() == ERROR_ALREADY_EXISTS { return None; }
            Some(handle as isize)
        }
    }
    #[cfg(not(target_os = "windows"))]
    { Some(0) }
}

/// ウィンドウタイトルを構築する (例: "Hinjaku - ProMode - OpenGL {custom.ini}")
pub fn build_window_title(config_name: Option<&str>, renderer: &crate::config::RendererMode, pro_mode: bool) -> String {
    let renderer_str = match renderer {
        crate::config::RendererMode::Glow => "OpenGL",
        crate::config::RendererMode::Wgpu => "Wgpu",
    };
    let pro_part = if pro_mode { "ProMode - " } else { "" };
    let config_part = config_name
        .filter(|&n| n != "config.ini")
        .map(|n| format!(" {{{}}}", n))
        .unwrap_or_default();
    format!("Hinjaku - {}{}{}", pro_part, renderer_str, config_part)
}

/// WindowsのGUIアプリとして起動しつつ、起動元のコンソールに出力できるようにする
pub fn setup_console() {
    #[cfg(target_os = "windows")]
    unsafe {
        if windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS) == 0 {
            windows_sys::Win32::System::Console::AllocConsole();
        }
    }

    // パニック発生時にコンソールが即座に閉じるのを防ぐフックを設定
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        default_hook(panic_info);
        eprintln!("\n--- APPLICATION PANIC ---");
        eprintln!("アプリケーションが異常終了しました。Enterキーを押すとこのウィンドウを閉じます...");
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
    }));
}