//! The Scenes tab: save and recall whole-mixer snapshots from the settings
//! directory. Like the TASCAM Settings Panel's scene memories, but stored as
//! ordinary preset files on the host, with no fixed limit on how many.

use std::path::PathBuf;

use eframe::egui;

use crate::app::{App, scene_label, scene_paths};

/// Render the scenes tab: a name field to save the current mix, and a list of
/// saved scenes to load or delete.
pub(crate) fn show(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Scenes");
    ui.label("Save and recall whole-mixer snapshots stored in your settings directory.");
    ui.add_space(4.0);

    // Save the current mixer as a new (or overwriting) scene.
    let mut save = false;
    ui.horizontal(|ui| {
        ui.label("Name:");
        let edit = ui.add(
            egui::TextEdit::singleline(&mut app.scene_name)
                .hint_text("scene name")
                .desired_width(180.0),
        );
        let entered = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button("Save current mix").clicked() || entered {
            save = true;
        }
    });
    if save {
        let name = app.scene_name.trim().to_owned();
        app.save_scene(&name); // reports an error itself if the name is empty
        if !name.is_empty() {
            app.scene_name.clear();
        }
    }

    ui.separator();

    // A pending delete shows a confirmation bar before the list.
    if let Some(path) = app.pending_delete.clone() {
        ui.horizontal(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(220, 120, 60),
                format!("Delete scene \u{201c}{}\u{201d}?", scene_label(&path)),
            );
            if ui.button("Delete").clicked() {
                app.delete_scene(&path);
                app.pending_delete = None;
            }
            if ui.button("Cancel").clicked() {
                app.pending_delete = None;
            }
        });
        ui.separator();
    }

    let scenes = scene_paths();
    if scenes.is_empty() {
        ui.label("No scenes saved yet.");
        return;
    }

    let mut to_save: Option<PathBuf> = None;
    let mut to_load: Option<PathBuf> = None;
    let mut to_delete: Option<PathBuf> = None;
    egui::Grid::new("scenes-grid")
        .striped(true)
        .num_columns(4)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            for path in &scenes {
                ui.label(scene_label(path));
                // Save: overwrite this scene with the current mixer.
                if ui
                    .button("Save")
                    .on_hover_text("Overwrite this scene with the current mixer")
                    .clicked()
                {
                    to_save = Some(path.clone());
                }
                if ui.button("Load").clicked() {
                    to_load = Some(path.clone());
                }
                if ui.button("Delete").clicked() {
                    to_delete = Some(path.clone());
                }
                ui.end_row();
            }
        });

    if let Some(path) = to_save {
        app.update_scene(&path);
    }
    if let Some(path) = to_load {
        app.load_scene(&path);
    }
    if let Some(path) = to_delete {
        app.pending_delete = Some(path);
    }
}
