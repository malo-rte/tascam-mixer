//! Hardware abstraction: the [`Backend`] trait and its implementations.
//!
//! [`Backend`] is the narrow seam between the typed device API and the actual
//! ALSA control elements. Keeping it a trait lets the whole library -- and the
//! tools built on it -- run against an in-memory [`MockBackend`] with no sound
//! card or `libasound` present (rust-coding-rules RS-80).

mod mock;
pub use mock::MockBackend;

#[cfg(feature = "alsa")]
mod alsa;
#[cfg(feature = "alsa")]
pub use alsa::AlsaBackend;

use crate::error::Result;

/// Raw, name-addressed access to the card's control elements.
///
/// Names are the resolved ALSA control names (the device layer handles any
/// name-spelling aliases before calling these). `index` selects the element
/// within a multi-instance control. Reads take `&self`; writes take `&mut self`.
pub trait Backend {
    /// Read integer slot 0 of the named element.
    fn get_int(&self, name: &str, index: u32) -> Result<i32>;
    /// Write integer slot 0 of the named element.
    fn set_int(&mut self, name: &str, index: u32, val: i32) -> Result<()>;
    /// Read boolean slot 0 of the named element.
    fn get_bool(&self, name: &str, index: u32) -> Result<bool>;
    /// Write boolean slot 0 of the named element.
    fn set_bool(&mut self, name: &str, index: u32, val: bool) -> Result<()>;
    /// Read up to `out.len()` integer slots of the named element into `out`,
    /// returning the number of slots written.
    fn get_ints(&self, name: &str, out: &mut [i32]) -> Result<usize>;
    /// The names of all control elements the backend currently knows about.
    ///
    /// Used by the device layer to resolve name-spelling aliases against what
    /// the loaded card actually exposes.
    fn elem_names(&self) -> Vec<String>;
}

/// Lets a `Us16x08<Box<dyn Backend>>` hold either backend chosen at runtime
/// (e.g. mock vs hardware behind a command-line flag) without a wrapper enum.
/// Generic over the boxed type, so it also covers `Box<dyn Backend + Send>`
/// (which a multi-threaded host needs to move/share the device).
impl<B: Backend + ?Sized> Backend for Box<B> {
    fn get_int(&self, name: &str, index: u32) -> Result<i32> {
        (**self).get_int(name, index)
    }
    fn set_int(&mut self, name: &str, index: u32, val: i32) -> Result<()> {
        (**self).set_int(name, index, val)
    }
    fn get_bool(&self, name: &str, index: u32) -> Result<bool> {
        (**self).get_bool(name, index)
    }
    fn set_bool(&mut self, name: &str, index: u32, val: bool) -> Result<()> {
        (**self).set_bool(name, index, val)
    }
    fn get_ints(&self, name: &str, out: &mut [i32]) -> Result<usize> {
        (**self).get_ints(name, out)
    }
    fn elem_names(&self) -> Vec<String> {
        (**self).elem_names()
    }
}
