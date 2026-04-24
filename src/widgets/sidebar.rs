use eframe::egui::{self, Color32, RichText, Layout, Align, ScrollArea};
use std::path::PathBuf;
use crate::nav_tree::NavTree;
use crate::utils;

pub fn sidebar_ui(
    ui: &mut egui::Ui,
    nav_tree: &mut NavTree,
    archive_path: &Option<PathBuf>,
    ctx: &egui::Context,
    open_req: &mut Option<PathBuf>,
) {
    ui.vertical(|ui| {
        ui.set_min_width(ui.available_width());
        ui.heading("ディレクトリツリー");
        ui.separator();

        let scroll_height = ui.available_height() - 40.0;
        ScrollArea::vertical().max_height(scroll_height).auto_shrink([false; 2]).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let roots = nav_tree.get_roots();
            for root in roots {
                ui_dir_tree(nav_tree, archive_path, ui, root, ctx, open_req);
            }
        });

        ui.separator();
        if let Some(sel) = nav_tree.selected.clone() {
            let count = nav_tree.get_image_count(&sel);
            ui.label(format!("選択中: {} ({}枚)", utils::get_display_name(&sel), count));
        }
    });
}

pub fn ui_dir_tree(
    nav_tree: &mut NavTree,
    current_path: &Option<PathBuf>,
    ui: &mut egui::Ui,
    path: PathBuf,
    ctx: &egui::Context,
    open_req: &mut Option<PathBuf>,
) {
    if utils::is_system(&path) || utils::is_hidden(&path) {
        return;
    }

    let filename = if path.parent().is_none() {
        path.to_string_lossy().to_string()
    } else {
        utils::get_display_name(&path)
    };

    let kind = utils::detect_kind(&path);
    let is_archive  = matches!(kind, utils::ArchiveKind::Zip | utils::ArchiveKind::SevenZ);
    let is_current  = current_path.as_ref() == Some(&path);
    let is_selected = nav_tree.selected.as_ref() == Some(&path);

    let text = RichText::new(filename);
    let text = if is_current  { text.color(Color32::YELLOW) } else { text };
    let text = if is_selected { text.background_color(ui.visuals().selection.bg_fill.linear_multiply(0.3)) } else { text };

    // egui の CollapsingHeader は ctx を要求しないが、将来の互換性のため引数として受け取っておく
    let _ = ctx;

    ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
        if is_archive {
            let resp = ui.selectable_label(is_selected, text);
            if is_selected && nav_tree.scroll_to_selected { resp.scroll_to_me(Some(egui::Align::Center)); }
            if resp.clicked() { *open_req = Some(path); }
        } else {
            let is_expanded = nav_tree.expanded.contains(&path);
            let response = egui::CollapsingHeader::new(text)
                .id_source(&path)
                .open(Some(is_expanded))
                .show(ui, |ui| {
                    let children = nav_tree.get_children(&path);
                    for p in children {
                        ui_dir_tree(nav_tree, current_path, ui, p, ctx, open_req);
                    }
                });
            if is_selected && nav_tree.scroll_to_selected {
                response.header_response.scroll_to_me(Some(egui::Align::Center));
            }
            if response.header_response.clicked() {
                nav_tree.selected = Some(path.clone());
                if is_expanded {
                    nav_tree.expanded.remove(&path);
                } else {
                    nav_tree.expanded.insert(path);
                }
                *open_req = nav_tree.selected.clone(); // selected は直前で Some に設定済み
            }
        }
    });
}
