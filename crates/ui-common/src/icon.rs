//! Action glyphs from the embedded Nerd Font (see [`crate::install_fonts`]).
//!
//! Used as the label of an [`crate::action_button`] for the compact per-row
//! actions the Rackctl GUIs share. An icon alone is ambiguous, so always pair one
//! with a text tooltip (`.on_hover_text(...)`). The project's canonical left-to-
//! right order for a row's actions is **Edit · Load · Save · Revert · Copy ·
//! Paste · Clear · Delete** — render each list as a subset, never reordered.

/// Pencil-in-box — edit / open in the editor.
pub const EDIT: &str = "\u{f044}";
/// Download tray — load / recall from a file or preset.
pub const LOAD: &str = "\u{f019}";
/// Floppy disk — save / overwrite / store.
pub const SAVE: &str = "\u{f0c7}";
/// Undo arrow — revert to the stored baseline.
pub const REVERT: &str = "\u{f0e2}";
/// Overlapping pages — copy to the clipboard.
pub const COPY: &str = "\u{f0c5}";
/// Clipboard — paste from the clipboard.
pub const PASTE: &str = "\u{f0ea}";
/// Eraser — clear to an empty/neutral state.
pub const CLEAR: &str = "\u{f12d}";
/// Trash can — delete permanently.
pub const DELETE: &str = "\u{f014}";
