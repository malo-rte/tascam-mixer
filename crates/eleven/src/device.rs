//! The [`Eleven`] device facade: a typed view over any [`Transport`].

use crate::backend::Transport;
use rackctl_eleven_model::error::Result;
use rackctl_eleven_model::value::RawValue;

/// A typed handle to an Eleven Rack, generic over its [`Transport`].
///
/// Reads and writes name a parameter by its raw address and exchange a decoded
/// word ([`u64`]) or the raw five-byte wire value ([`RawValue`]). What the word
/// means per parameter is still being decoded, so the facade stays at the
/// address/word level; the typed-by-name catalog layers on top later.
#[derive(Debug)]
pub struct Eleven<T> {
    transport: T,
}

impl<T: Transport> Eleven<T> {
    /// Wrap a transport.
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    /// Read the parameter at `addr`, returning the decoded 35-bit word.
    ///
    /// # Errors
    /// Propagates the transport's error (e.g. [`rackctl_eleven_model::Error::Timeout`]
    /// if the unit does not reply).
    pub fn read(&mut self, addr: &[u8]) -> Result<u64> {
        Ok(self.transport.read(addr)?.decode())
    }

    /// Read the parameter at `addr`, returning the raw five-byte wire value.
    ///
    /// # Errors
    /// Propagates the transport's error.
    pub fn read_raw(&mut self, addr: &[u8]) -> Result<RawValue> {
        self.transport.read(addr)
    }

    /// Write `word` to the parameter at `addr`.
    ///
    /// # Errors
    /// Propagates the transport's error.
    pub fn write(&mut self, addr: &[u8], word: u64) -> Result<()> {
        self.transport.write(addr, &RawValue::encode(word))
    }

    /// Write a raw five-byte wire value to the parameter at `addr`.
    ///
    /// # Errors
    /// Propagates the transport's error.
    pub fn write_raw(&mut self, addr: &[u8], value: &RawValue) -> Result<()> {
        self.transport.write(addr, value)
    }

    /// Read a batch of addresses, returning one `(address, value)` pair for each
    /// that answered (non-answering addresses are omitted). Over hardware this is
    /// a single batched request/collect — the basis for a `scan`/`dump`.
    ///
    /// # Errors
    /// Propagates the transport's error.
    pub fn scan(&mut self, addrs: &[Vec<u8>]) -> Result<Vec<(Vec<u8>, RawValue)>> {
        self.transport.scan(addrs)
    }

    /// Consume the facade and return the underlying transport.
    pub fn into_transport(self) -> T {
        self.transport
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::backend::MockTransport;

    #[test]
    fn read_after_write_decodes_the_word() {
        let mut dev = Eleven::new(MockTransport::new());
        let addr = [0x11, 0x21, 0x0D];
        dev.write(&addr, 0x6D).unwrap();
        assert_eq!(dev.read(&addr).unwrap(), 0x6D);
        assert_eq!(dev.read_raw(&addr).unwrap().as_bytes(), &[0x6D, 0, 0, 0, 0]);
    }

    #[test]
    fn scan_returns_written_addresses() {
        let mut dev = Eleven::new(MockTransport::new());
        dev.write(&[0x11, 0x21, 0x00], 0x40).unwrap();
        dev.write(&[0x11, 0x21, 0x0D], 0x6D).unwrap();
        let addrs = vec![vec![0x11, 0x21, 0x00], vec![0x11, 0x21, 0x0D]];
        let got = dev.scan(&addrs).unwrap();
        assert_eq!(
            got,
            vec![
                (vec![0x11, 0x21, 0x00], RawValue::encode(0x40)),
                (vec![0x11, 0x21, 0x0D], RawValue::encode(0x6D)),
            ]
        );
    }
}
