use std::path::Path;
use crate::{utils, integrator, manager::Manager, config::ExternalAppConfig};

/// 指定された外部アプリ設定に基づいて現在のファイルを開く
pub fn open_external(manager: &Manager, app: &ExternalAppConfig) -> Result<(), String> {
    let Some(virtual_str) = manager.get_current_full_path() else { return Ok(()) };
    let path_v = utils::to_clean_string(Path::new(&virtual_str)); // %P: 仮想パス

    let path_p = if let Some(base) = manager.archive_path.as_ref() {
        if utils::detect_kind(base) == utils::ArchiveKind::Plain {
            // 通常フォルダ内：画像ファイルそのものが物理パス
            let p = if base.is_dir() {
                let entry = manager.entries.get(manager.target_index).map(|s| s.as_str()).unwrap_or("");
                base.join(entry)
            } else {
                base.clone()
            };
            utils::to_clean_string(&p)
        } else {
            // アーカイブ内：アーカイブファイル自体が物理パス
            utils::to_clean_string(base)
        }
    } else {
        String::new()
    };

    integrator::launch_external(&app.exe, &app.args, &path_v, &path_p)
}

/// 現在表示しているファイル（または書庫）をエクスプローラーで選択状態で表示する
pub fn reveal_current_in_explorer(manager: &Manager) -> Result<(), String> {
    let Some(base) = &manager.archive_path else { return Ok(()) };
    
    let target = if utils::detect_kind(base) == utils::ArchiveKind::Plain && base.is_dir() {
        let entry = manager.entries.get(manager.target_index).map(|s| s.as_str()).unwrap_or("");
        base.join(entry)
    } else {
        base.clone()
    };

    reveal_in_explorer(&target)
}

/// エクスプローラーで対象のパスを選択状態にする
fn reveal_in_explorer(path: &Path) -> Result<(), String> {
    if !path.exists() { return Err("対象のパスが見つかりません。".to_string()); }

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let operation: Vec<u16> = "open\0".encode_utf16().collect();
        let explorer: Vec<u16> = "explorer.exe\0".encode_utf16().collect();

        // エクスプローラーは UNCパスを受け付けないため、確実にクリーンな形式で渡す
        let cleaned = crate::utils::to_clean_string(path);
        let parameters: Vec<u16> = format!("/select,\"{}\"\0", cleaned).encode_utf16().collect();

        unsafe {
            let ret = ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                explorer.as_ptr(),
                parameters.as_ptr(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
            if (ret as isize) <= 32 {
                return Err(format!("エクスプローラーの起動に失敗しました (Code: {})", ret as isize));
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        match std::process::Command::new("open").arg(path).spawn() {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("エクスプローラー起動失敗: {}", e)),
        }
    }
}