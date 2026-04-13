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

    eframe::run_native(
        "ArchView",
        options,
        Box::new(|cc| Box::new(viewer::App::new(cc))),
    )
}
