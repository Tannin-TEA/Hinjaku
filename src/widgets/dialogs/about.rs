use eframe::egui::{self, RichText, Layout, Align};

pub fn about_window(ctx: &egui::Context, show: &mut bool) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { *show = false; }
    egui::Window::new("Hinjaku について")
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .fixed_size([450.0, 520.0])
        .collapsible(false)
        .resizable(true)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Hinjaku").size(28.0).strong());
                ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).size(16.0));
                ui.label(RichText::new("吹けば飛ぶよな軽量ビューア").size(15.0));
                ui.add_space(4.0);
                ui.hyperlink_to("GitHub リポジトリ", "https://github.com/Tannin-TEA/Hinjaku");
            });
            ui.separator();
            ui.label(RichText::new("使用しているオープンソースライブラリ:").size(14.0));
            ui.add_space(4.0);

            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                let licenses: &[(&str, &str, &str)] = &[
                    ("eframe / egui",  "MIT / Apache-2.0", "https://github.com/emilk/egui"),
                    ("image",          "MIT / Apache-2.0", "https://github.com/image-rs/image"),
                    ("zip",            "MIT",              "https://github.com/zip-rs/zip2"),
                    ("sevenz-rust",    "Apache-2.0",       "https://github.com/dyu/sevenz-rust"),
                    ("rust-ini",       "MIT",              "https://github.com/amrayn/rust-ini"),
                    ("windows-rs",     "MIT / Apache-2.0", "https://github.com/microsoft/windows-rs"),
                    ("rfd",            "MIT",              "https://github.com/PolyMeilex/rfd"),
                    ("pdfium-render",   "MIT",              "https://github.com/ajrcarey/pdfium-render"),
                    ("PDFium (公式)",   "BSD 3-Clause",    "https://pdfium.googlesource.com/pdfium/"),
                    ("PDFium (DL先)",   "BSD 3-Clause",    "https://github.com/bblanchon/pdfium-binaries"),
                ];
                for (name, license, url) in licenses {
                    ui.horizontal(|ui| {
                        ui.hyperlink_to(RichText::new(*name).size(14.0).strong(), *url);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(*license).size(12.0).weak());
                        });
                    });
                    ui.add_space(2.0);
                }
            });

            ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                ui.add_space(16.0);
                if ui.button(RichText::new("閉じる").size(16.0)).clicked() { *show = false; }
                ui.add_space(16.0);
                ui.separator();
            });
        });
}
