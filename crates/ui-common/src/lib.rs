//! Shared UI helpers for the Rackctl GUIs.
//!
//! The headline item is the project-wide **button colour scheme**: buttons are
//! coloured by the *consequence* of their action, so every tool reads the same way
//! at a glance and red keeps its "stop and think" weight. Use [`action_button`]
//! with the [`ActionKind`] that matches what the button does.
#![forbid(unsafe_code)]

pub mod comp;
pub mod eq;

use egui::{Button, Color32, Response, Ui, WidgetText};

/// Semantic category of a button, mapped to a fill colour so a GUI signals an
/// action's *consequence* (commit vs. read vs. discard vs. destroy) consistently.
///
/// This is the project-wide convention for **every** Rackctl GUI — don't invent
/// per-app colours; pick the kind that matches the action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    /// Persists edits / writes to the device (Save, Write, Apply, Load-to-device).
    /// Green.
    Commit,
    /// Pulls data in; changes nothing (Refresh, Reconnect, Backup, Load-from-file).
    /// Blue.
    Read,
    /// Undoes *your own unsaved edits*, back to what's already stored (Revert,
    /// Reset, Discard changes). Amber. The dividing line vs. `Destructive`: Caution
    /// only loses edits you just made; the underlying data is still on the device.
    Caution,
    /// Destroys *existing content*, not just your edits (Clear/blank a patch,
    /// Delete, Factory reset). Red — reserved for genuine destruction; nothing
    /// routine should use it. Still applies when the destruction is staged and
    /// undoable (e.g. a Clear that a Revert could restore): the *intent* is to
    /// throw the content away, so it reads red.
    Destructive,
    /// No consequence (Cancel, Close, Paste-into-buffer). The egui default, no tint.
    Neutral,
}

impl ActionKind {
    /// The muted dark-theme fill for this kind, or `None` to keep egui's default
    /// button colour.
    #[must_use]
    pub fn fill(self) -> Option<Color32> {
        match self {
            ActionKind::Commit => Some(Color32::from_rgb(46, 120, 75)),
            ActionKind::Read => Some(Color32::from_rgb(45, 95, 145)),
            ActionKind::Caution => Some(Color32::from_rgb(155, 110, 30)),
            ActionKind::Destructive => Some(Color32::from_rgb(150, 50, 50)),
            ActionKind::Neutral => None,
        }
    }
}

/// Add a button whose fill encodes its [`ActionKind`]. Returns the [`Response`] so
/// callers can chain `.on_hover_text(..)` and test `.clicked()` as usual.
///
/// Disabled buttons keep egui's dimming, so a colour does not make a disabled
/// button look active. Don't rely on colour alone — pair with enable/disable state
/// and hover tooltips.
pub fn action_button(ui: &mut Ui, label: impl Into<WidgetText>, kind: ActionKind) -> Response {
    let mut button = Button::new(label);
    if let Some(fill) = kind.fill() {
        button = button.fill(fill);
    }
    ui.add(button)
}
