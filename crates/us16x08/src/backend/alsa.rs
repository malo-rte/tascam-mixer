//! The real hardware [`Backend`], backed by ALSA's HCTL interface.
//!
//! This is the direct Rust counterpart of the C++ `OAlsa` class: card
//! discovery by ALSA id, opening the HCTL handle, and reading/writing control
//! elements by name + index.

use std::ffi::CString;

use ::alsa::ctl::{Ctl, ElemId, ElemIface};
use ::alsa::hctl::HCtl;

use super::Backend;
use crate::error::{Error, Result};

/// The ALSA card id the kernel `snd-usb-audio` driver assigns to this device.
const CARD_ID: &str = "US16x08";

/// A live connection to a US-16x08 via ALSA HCTL.
pub struct AlsaBackend {
    hctl: HCtl,
}

impl std::fmt::Debug for AlsaBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `HCtl` is an opaque handle with no Debug; identify the type only.
        f.debug_struct("AlsaBackend").finish_non_exhaustive()
    }
}

fn backend_err(e: ::alsa::Error) -> Error {
    Error::Backend(e.to_string())
}

impl AlsaBackend {
    /// Find the US-16x08 card, open its HCTL handle, and load the element list.
    ///
    /// # Errors
    /// Returns [`Error::CardNotFound`] if no card with ALSA id `US16x08` is
    /// present, or [`Error::Backend`] if ALSA reports an error while opening or
    /// loading the device.
    pub fn open() -> Result<Self> {
        let card = Self::find_card()?;
        let hctl = HCtl::new(&format!("hw:{card}"), false).map_err(backend_err)?;
        hctl.load().map_err(backend_err)?;
        Ok(Self { hctl })
    }

    /// Locate the card index whose ALSA id matches [`CARD_ID`].
    fn find_card() -> Result<i32> {
        for card in ::alsa::card::Iter::new() {
            let card = card.map_err(backend_err)?;
            let index = card.get_index();
            let ctl = Ctl::new(&format!("hw:{index}"), false).map_err(backend_err)?;
            let info = ctl.card_info().map_err(backend_err)?;
            if info.get_id().map_err(backend_err)? == CARD_ID {
                return Ok(index);
            }
        }
        Err(Error::CardNotFound)
    }

    /// Wait up to `timeout_ms` for the card to report control changes.
    ///
    /// Returns `true` if events are pending. Pair with [`Self::handle_events`]
    /// and a re-read of the controls of interest to observe external changes
    /// (e.g. front-panel knobs or another client).
    ///
    /// # Errors
    /// [`Error::Backend`] if ALSA reports a poll error.
    pub fn wait(&self, timeout_ms: Option<u32>) -> Result<bool> {
        self.hctl.wait(timeout_ms).map_err(backend_err)
    }

    /// Dispatch pending HCTL events, returning the number handled.
    ///
    /// # Errors
    /// [`Error::Backend`] if ALSA reports an error.
    pub fn handle_events(&self) -> Result<u32> {
        self.hctl.handle_events().map_err(backend_err)
    }

    fn find(&self, name: &str, index: u32) -> Result<::alsa::hctl::Elem<'_>> {
        let cname = CString::new(name)
            .map_err(|_| Error::Backend("control name contains NUL".to_owned()))?;
        let mut id = ElemId::new(ElemIface::Mixer);
        id.set_name(&cname);
        id.set_index(index);
        self.hctl
            .find_elem(&id)
            .ok_or_else(|| Error::UnknownControl {
                name: name.to_owned(),
                index,
            })
    }
}

impl Backend for AlsaBackend {
    fn get_int(&self, name: &str, index: u32) -> Result<i32> {
        let elem = self.find(name, index)?;
        let value = elem.read().map_err(backend_err)?;
        // Enumerated controls (e.g. "Line Out Route", "Compressor Ratio") store
        // their selection in a separate union member from plain integers; read
        // whichever this element actually is, mapping the enum item to its index.
        if let Some(int) = value.get_integer(0) {
            return Ok(int);
        }
        if let Some(item) = value.get_enumerated(0) {
            return Ok(i32::try_from(item).unwrap_or(i32::MAX));
        }
        Err(Error::Backend(format!(
            "control {name:?} is neither an integer nor an enumerated control"
        )))
    }

    fn set_int(&mut self, name: &str, index: u32, val: i32) -> Result<()> {
        let elem = self.find(name, index)?;
        // Read first to obtain a correctly typed ElemValue, then mutate slot 0,
        // choosing the integer or enumerated accessor to match the element.
        let mut value = elem.read().map_err(backend_err)?;
        if value.set_integer(0, val).is_none() {
            let item = u32::try_from(val)
                .map_err(|_| Error::Backend(format!("negative value {val} for an enum control")))?;
            value.set_enumerated(0, item).ok_or_else(|| {
                Error::Backend(format!(
                    "control {name:?} is neither an integer nor an enumerated control"
                ))
            })?;
        }
        elem.write(&value).map_err(backend_err)?;
        Ok(())
    }

    fn get_bool(&self, name: &str, index: u32) -> Result<bool> {
        let elem = self.find(name, index)?;
        let value = elem.read().map_err(backend_err)?;
        value.get_boolean(0).ok_or(Error::TypeMismatch {
            control: "<elem>",
            expected: "boolean",
        })
    }

    fn set_bool(&mut self, name: &str, index: u32, val: bool) -> Result<()> {
        let elem = self.find(name, index)?;
        let mut value = elem.read().map_err(backend_err)?;
        value.set_boolean(0, val).ok_or(Error::TypeMismatch {
            control: "<elem>",
            expected: "boolean",
        })?;
        elem.write(&value).map_err(backend_err)?;
        Ok(())
    }

    fn get_ints(&self, name: &str, out: &mut [i32]) -> Result<usize> {
        let elem = self.find(name, 0)?;
        let value = elem.read().map_err(backend_err)?;
        let mut n = 0;
        for (slot, dst) in out.iter_mut().enumerate() {
            // `out` is a small fixed buffer (METER_COUNT); the cast cannot truncate.
            #[allow(clippy::cast_possible_truncation)]
            match value.get_integer(slot as u32) {
                Some(v) => {
                    *dst = v;
                    n += 1;
                }
                None => break,
            }
        }
        Ok(n)
    }

    fn elem_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for elem in self.hctl.elem_iter() {
            if let Ok(id) = elem.get_id()
                && let Ok(name) = id.get_name()
            {
                names.push(name.to_owned());
            }
        }
        names.sort_unstable();
        names.dedup();
        names
    }
}
