//! An in-memory [`Transport`] for development and tests.

use std::collections::BTreeMap;

use super::Transport;
use crate::error::Result;
use crate::param::{self, Kind};

/// In-memory stand-in for a real GX-700 over MIDI.
///
/// Seeded from [`param::ALL`] with each parameter's default byte at its address,
/// so the full parameter surface is present without hardware. [`Transport::send`]
/// stores bytes, [`Transport::request`] returns them (zero-padded to the
/// requested length), and [`Transport::program_change`] records the selection.
#[derive(Debug, Clone)]
pub struct MockTransport {
    store: BTreeMap<Vec<u8>, Vec<u8>>,
    program: u8,
}

impl MockTransport {
    /// Build a mock seeded with every parameter at its default value.
    #[must_use]
    pub fn new() -> Self {
        let mut store = BTreeMap::new();
        for p in param::ALL {
            let default: u8 = match p.kind() {
                Kind::Bool => 0,
                Kind::Int { default, .. } | Kind::Enum { default, .. } => {
                    u8::try_from(default).unwrap_or(0)
                }
            };
            store.insert(p.address().to_vec(), vec![default]);
        }
        Self { store, program: 0 }
    }

    /// The most recently selected program number.
    #[must_use]
    pub fn program(&self) -> u8 {
        self.program
    }
}

impl Default for MockTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for MockTransport {
    fn send(&mut self, addr: &[u8], data: &[u8]) -> Result<()> {
        self.store.insert(addr.to_vec(), data.to_vec());
        Ok(())
    }

    fn request(&mut self, addr: &[u8], len: usize) -> Result<Vec<u8>> {
        let mut out = self.store.get(addr).cloned().unwrap_or_default();
        out.resize(len, 0);
        Ok(out)
    }

    fn request_blocks(&mut self, addr: &[u8], _size: usize) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        // Return every stored entry in the same area+patch (first two address
        // bytes), mirroring how the hardware streams a whole patch's sub-blocks.
        let prefix = addr.get(..2).unwrap_or(addr);
        Ok(self
            .store
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }

    fn program_change(&mut self, program: u8) -> Result<()> {
        self.program = program & 0x7f;
        Ok(())
    }
}
