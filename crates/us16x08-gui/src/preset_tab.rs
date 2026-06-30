//! A list tab for named, file-backed presets. The same UI serves the *Scenes*
//! tab (whole-mixer snapshots) and the *Channel presets* tab (single-channel
//! strips applied to the focused channel); see [`PresetKind`].

use std::path::PathBuf;

use eframe::egui;
use rackctl_ui::{ActionKind, action_button, icon};

use crate::app::{App, PresetKind, preset_label, preset_paths};

/// Render the preset tab for `kind`: a name field to save the current state, and
/// a list of saved presets to update, load, or delete.
pub(crate) fn show(app: &mut App, ui: &mut egui::Ui, kind: PresetKind) {
    heading(app, ui, kind);
    ui.add_space(4.0);

    // Save the current state as a new (or overwriting) preset.
    let mut save = false;
    ui.horizontal(|ui| {
        ui.label("Name:");
        let edit = ui.add(
            egui::TextEdit::singleline(app.preset_name_mut(kind))
                .hint_text("preset name")
                .desired_width(180.0),
        );
        let entered = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let label = match kind {
            PresetKind::Scene => "Save current mix",
            PresetKind::Strip => "Save this channel",
            PresetKind::Eq => "Save this EQ",
            PresetKind::Comp => "Save this compressor",
        };
        if action_button(ui, label, ActionKind::Commit).clicked() || entered {
            save = true;
        }
    });
    if save {
        let name = app.preset_name_mut(kind).trim().to_owned();
        app.save_named_preset(kind, &name); // reports an error itself if empty
        if !name.is_empty() {
            app.preset_name_mut(kind).clear();
        }
    }

    ui.separator();

    // A pending delete for this tab's directory shows a confirmation bar first.
    if let Some(path) = app.pending_delete.clone()
        && path.parent() == App::preset_dir(kind).as_deref()
    {
        ui.horizontal(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(220, 120, 60),
                format!("Delete \u{201c}{}\u{201d}?", preset_label(&path)),
            );
            if action_button(ui, "Delete", ActionKind::Destructive).clicked() {
                app.delete_preset(&path);
                app.pending_delete = None;
            }
            if action_button(ui, "Cancel", ActionKind::Neutral).clicked() {
                app.pending_delete = None;
            }
        });
        ui.separator();
    }

    let presets = preset_paths(kind);
    if presets.is_empty() {
        ui.label("Nothing saved yet.");
        return;
    }

    let mut to_save: Option<PathBuf> = None;
    let mut to_load: Option<PathBuf> = None;
    let mut to_copy: Option<PathBuf> = None;
    let mut to_delete: Option<PathBuf> = None;
    let overwrite_hint = match kind {
        PresetKind::Scene => "Overwrite this scene with the current mixer",
        PresetKind::Strip => "Overwrite this preset with the current channel",
        PresetKind::Eq => "Overwrite this preset with the current EQ",
        PresetKind::Comp => "Overwrite this preset with the current compressor",
    };
    // The per-channel kinds can be copied into the paste clipboard.
    let copyable = kind.clipboard_group().is_some();
    egui::Grid::new("preset-grid")
        .striped(true)
        .num_columns(if copyable { 5 } else { 4 })
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            for path in &presets {
                // Action icons first (left), in the canonical order
                // (Load · Save · Copy · Delete); the name follows. Each icon keeps
                // a tooltip, matching the GX-700 GUI's list rows.
                if action_button(ui, icon::LOAD, ActionKind::Read)
                    .on_hover_text("Load this preset")
                    .clicked()
                {
                    to_load = Some(path.clone());
                }
                if action_button(ui, icon::SAVE, ActionKind::Commit)
                    .on_hover_text(overwrite_hint)
                    .clicked()
                {
                    to_save = Some(path.clone());
                }
                if copyable
                    && action_button(ui, icon::COPY, ActionKind::Read)
                        .on_hover_text("Copy to the clipboard, then Paste onto a channel")
                        .clicked()
                {
                    to_copy = Some(path.clone());
                }
                if action_button(ui, icon::DELETE, ActionKind::Destructive)
                    .on_hover_text("Delete this preset")
                    .clicked()
                {
                    to_delete = Some(path.clone());
                }
                ui.label(preset_label(path));
                ui.end_row();
            }
        });

    if let Some(path) = to_save {
        app.update_named_preset(kind, &path);
    }
    if let Some(path) = to_load {
        app.load_named_preset(kind, &path);
    }
    if let Some(path) = to_copy {
        app.copy_preset(kind, &path);
    }
    if let Some(path) = to_delete {
        app.pending_delete = Some(path);
    }
}

/// The tab's heading and one-line description.
fn heading(app: &App, ui: &mut egui::Ui, kind: PresetKind) {
    match kind {
        PresetKind::Scene => {
            ui.heading("Scenes");
            ui.label("Save and recall whole-mixer snapshots stored in your settings directory.");
        }
        PresetKind::Strip => {
            ui.heading("Channel presets");
            ui.label(format!(
                "Save and recall one channel's settings. Save captures channel {}, \
                 and Load applies a preset to it.",
                app.selected + 1
            ));
        }
        PresetKind::Eq => {
            ui.heading("EQ presets");
            ui.label(format!(
                "Save and recall a channel's EQ section. Save captures channel {}'s EQ, \
                 and Load applies a preset to it.",
                app.selected + 1
            ));
        }
        PresetKind::Comp => {
            ui.heading("Compressor presets");
            ui.label(format!(
                "Save and recall a channel's compressor. Save captures channel {}'s \
                 compressor, and Load applies a preset to it.",
                app.selected + 1
            ));
        }
    }
}
