//! The device's long-term identity.
//!
//! Each installation owns one X25519 key pair, created on first launch and kept
//! in `identity.key` (owner read/write only, like an SSH key). The public half
//! *is* the device identity: pairing records it, and every later connection must
//! prove possession of the private half during the Noise handshake.

use blake2::{Blake2s256, Digest};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::protocol::NOISE_PATTERN;

const FILE: &str = "identity.key";

/// A device's public key, which is also its stable identifier.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PeerId(pub [u8; 32]);

impl PeerId {
    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        bytes.try_into().ok().map(PeerId)
    }

    pub fn to_hex(&self) -> String {
        hex(&self.0)
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        if s.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let pair = std::str::from_utf8(chunk).ok()?;
            out[i] = u8::from_str_radix(pair, 16).ok()?;
        }
        Some(PeerId(out))
    }

    /// 16 hex characters advertised over DNS-SD so a browser can recognise a
    /// trusted device before connecting. Only a hint: trust is always checked
    /// against the full key in the handshake.
    pub fn fingerprint(&self) -> String {
        let digest = Blake2s256::digest(self.0);
        hex(&digest[..8])
    }

    /// Human-friendly form shown in the UI, e.g. `3F2A-91C0`.
    pub fn short(&self) -> String {
        let f = self.fingerprint().to_uppercase();
        format!("{}-{}", &f[..4], &f[4..8])
    }
}

impl fmt::Debug for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PeerId({})", self.short())
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 0xF) as usize] as char);
    }
    s
}

/// The local key pair.
#[derive(Clone)]
pub struct Identity {
    private: [u8; 32],
    public: [u8; 32],
}

impl Identity {
    pub fn generate() -> io::Result<Self> {
        let builder = snow::Builder::new(NOISE_PATTERN.parse().expect("valid noise pattern"));
        let kp = builder.generate_keypair().map_err(|e| io::Error::other(e.to_string()))?;
        Ok(Identity {
            private: kp.private.as_slice().try_into().map_err(|_| io::Error::other("bad key length"))?,
            public: kp.public.as_slice().try_into().map_err(|_| io::Error::other("bad key length"))?,
        })
    }

    /// Loads the identity from `dir`, creating and saving a new one if absent.
    pub fn load_or_create(dir: &Path) -> io::Result<Self> {
        let path = dir.join(FILE);
        match fs::read(&path) {
            Ok(bytes) if bytes.len() == 64 => {
                let mut private = [0u8; 32];
                let mut public = [0u8; 32];
                private.copy_from_slice(&bytes[..32]);
                public.copy_from_slice(&bytes[32..]);
                Ok(Identity { private, public })
            }
            Ok(_) => Err(io::Error::new(io::ErrorKind::InvalidData, "identity.key is corrupt")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let id = Identity::generate()?;
                fs::create_dir_all(dir)?;
                let tmp = dir.join(".identity.key.tmp");
                {
                    let mut f = private_file(&tmp)?;
                    f.write_all(&id.private)?;
                    f.write_all(&id.public)?;
                    f.sync_all()?;
                }
                fs::rename(&tmp, &path)?;
                Ok(id)
            }
            Err(e) => Err(e),
        }
    }

    pub fn private_key(&self) -> &[u8; 32] {
        &self.private
    }

    pub fn id(&self) -> PeerId {
        PeerId(self.public)
    }
}

#[cfg(unix)]
fn private_file(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)
}

#[cfg(not(unix))]
fn private_file(path: &Path) -> io::Result<fs::File> {
    // The per-user profile directory is already private on Windows.
    fs::OpenOptions::new().write(true).create(true).truncate(true).open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_persists_across_loads() {
        let dir = std::env::temp_dir().join(format!("edgeway-id-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let a = Identity::load_or_create(&dir).unwrap();
        let b = Identity::load_or_create(&dir).unwrap();
        assert_eq!(a.id(), b.id());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join(FILE)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hex_round_trips_and_fingerprints_are_stable() {
        let id = Identity::generate().unwrap().id();
        assert_eq!(PeerId::from_hex(&id.to_hex()), Some(id));
        assert_eq!(id.fingerprint().len(), 16);
        assert_eq!(id.short().len(), 9);
        assert!(PeerId::from_hex("zz").is_none());
    }
}
