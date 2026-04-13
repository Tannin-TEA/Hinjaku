#![windows_subsystem = "windows"]

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

    // 複数起動のチェック (Windows用)
    #[cfg(target_os = "windows")]
    let _mutex = if !config.allow_multiple_instances {
        use windows_sys::Win32::System::Threading::{CreateMutexW, GetLastError};
        use windows_sys::Win32::Foundation::ERROR_ALREADY_EXISTS;
        let name: Vec<u16> = "ArchView_SingleInstance_Mutex\0".encode_utf16().collect();
        let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            return Ok(());
        }
        Some(handle)
    } else {
        None
    };

    let args: Vec<String> = std::env::args().collect();
    let initial_path = args.get(1).map(std::path::PathBuf::from);

    eframe::run_native(
        "ArchView",
        options,
        Box::new(move |cc| Box::new(viewer::App::new(cc, initial_path))),
    )
}
