//! Display-unit formatting for parameter values.
//!
//! Turns a raw device [`Value`] into the human-readable form shown to the user.
//! Most parameters display as their raw number or enum label; a few carry a unit
//! (a percentage, dB, Hz, ms), added here. The catalog ranges stay raw -- this is
//! the presentation layer, extended one parameter family at a time.

use crate::param::{Kind, Param, Value};

/// Format `value` for `param` in display units.
#[must_use]
pub fn display(param: Param, value: Value) -> String {
    match value {
        Value::Bool(on) => if on { "on" } else { "off" }.to_owned(),
        Value::Int(raw) => match param.key() {
            // Output level is a 0..=100 level; show it as a percentage.
            "output-level" => format!("{raw}%"),
            _ => raw.to_string(),
        },
        Value::Enum(index) => enum_label(param, index),
    }
}

/// The label for an enum `index`, or the bare index if out of range / not an enum.
fn enum_label(param: Param, index: i32) -> String {
    if let Kind::Enum { values, .. } = param.kind()
        && let Some(label) = usize::try_from(index).ok().and_then(|i| values.get(i))
    {
        return (*label).to_owned();
    }
    index.to_string()
}
