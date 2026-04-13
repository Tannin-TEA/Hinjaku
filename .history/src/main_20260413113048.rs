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

    let args: Vec<String> = std::env::args().collect();
    let initial_path = args.get(1).map(std::path::PathBuf::from);

    eframe::run_native(
        "ArchView",
        options,
        Box::new(move |cc| Box::new(viewer::App::new(cc, initial_path))),
    )
}
