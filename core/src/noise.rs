//! Encrypted, authenticated transport.
//!
//! Every connection starts with a Noise `XX` handshake
//! (`Noise_XX_25519_ChaChaPoly_BLAKE2s`, prologue `edgeway/1`). It gives both
//! sides forward secrecy and each other's long-term public key; the engine
//! then decides whether that key is trusted. Afterwards each message travels in
//! one frame: a big-endian `u16` length followed by the ciphertext.
//!
//! The transport state is split into a [`Sealer`] and an [`Opener`] that each
//! keep their own nonce counter, so the writer and reader threads never contend.

use snow::StatelessTransportState;
use std::io::{self, Read, Write};
use std::sync::Arc;

use crate::identity::{Identity, PeerId};
use crate::protocol::{Message, NOISE_PATTERN, NOISE_PROLOGUE};

/// Largest Noise message.
pub const MAX_FRAME: usize = 65535;
const TAG_LEN: usize = 16;

pub fn write_frame<W: Write>(w: &mut W, data: &[u8]) -> io::Result<()> {
    if data.len() > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "frame too large"));
    }
    w.write_all(&(data.len() as u16).to_be_bytes())?;
    w.write_all(data)
}

pub fn read_frame<R: Read>(r: &mut R) -> io::Result<Vec<u8>> {
    let mut len = [0u8; 2];
    r.read_exact(&mut len)?;
    let len = u16::from_be_bytes(len) as usize;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

fn noise_err(e: snow::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("noise: {e}"))
}

/// Result of a completed handshake.
pub struct Handshake {
    pub remote: PeerId,
    /// Hash of the whole handshake transcript; binds pairing to this session.
    pub hash: [u8; 32],
    pub sealer: Sealer,
    pub opener: Opener,
}

fn builder(identity: &Identity) -> Result<snow::Builder<'_>, snow::Error> {
    snow::Builder::new(NOISE_PATTERN.parse()?)
        .local_private_key(identity.private_key())?
        .prologue(NOISE_PROLOGUE)
}

fn finish(hs: snow::HandshakeState) -> io::Result<Handshake> {
    let remote = hs
        .get_remote_static()
        .and_then(PeerId::from_slice)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "peer sent no static key"))?;
    let hash: [u8; 32] = hs
        .get_handshake_hash()
        .try_into()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "unexpected hash length"))?;
    let transport = Arc::new(hs.into_stateless_transport_mode().map_err(noise_err)?);
    Ok(Handshake {
        remote,
        hash,
        sealer: Sealer { transport: transport.clone(), nonce: 0 },
        opener: Opener { transport, nonce: 0 },
    })
}

/// Runs the initiator side: `-> e`, `<- e, ee, s, es`, `-> s, se`.
pub fn initiate<S: Read + Write>(stream: &mut S, identity: &Identity) -> io::Result<Handshake> {
    let mut hs = builder(identity).and_then(|b| b.build_initiator()).map_err(noise_err)?;
    let mut buf = vec![0u8; MAX_FRAME];
    let n = hs.write_message(&[], &mut buf).map_err(noise_err)?;
    write_frame(stream, &buf[..n])?;
    let msg = read_frame(stream)?;
    hs.read_message(&msg, &mut buf).map_err(noise_err)?;
    let n = hs.write_message(&[], &mut buf).map_err(noise_err)?;
    write_frame(stream, &buf[..n])?;
    stream.flush()?;
    finish(hs)
}

/// Runs the responder side of the same handshake.
pub fn respond<S: Read + Write>(stream: &mut S, identity: &Identity) -> io::Result<Handshake> {
    let mut hs = builder(identity).and_then(|b| b.build_responder()).map_err(noise_err)?;
    let mut buf = vec![0u8; MAX_FRAME];
    let msg = read_frame(stream)?;
    hs.read_message(&msg, &mut buf).map_err(noise_err)?;
    let n = hs.write_message(&[], &mut buf).map_err(noise_err)?;
    write_frame(stream, &buf[..n])?;
    stream.flush()?;
    let msg = read_frame(stream)?;
    hs.read_message(&msg, &mut buf).map_err(noise_err)?;
    finish(hs)
}

/// Encrypts outgoing messages.
pub struct Sealer {
    transport: Arc<StatelessTransportState>,
    nonce: u64,
}

impl Sealer {
    pub fn seal(&mut self, plaintext: &[u8]) -> io::Result<Vec<u8>> {
        let mut out = vec![0u8; plaintext.len() + TAG_LEN];
        let n = self.transport.write_message(self.nonce, plaintext, &mut out).map_err(noise_err)?;
        self.nonce += 1;
        out.truncate(n);
        Ok(out)
    }

    /// Encrypts and frames a message.
    pub fn send<W: Write>(&mut self, w: &mut W, msg: &Message) -> io::Result<()> {
        let sealed = self.seal(&msg.encode())?;
        write_frame(w, &sealed)
    }
}

/// Decrypts incoming messages. Any tampering, replay or reordering fails
/// authentication and ends the connection.
pub struct Opener {
    transport: Arc<StatelessTransportState>,
    nonce: u64,
}

impl Opener {
    pub fn open(&mut self, ciphertext: &[u8]) -> io::Result<Vec<u8>> {
        let mut out = vec![0u8; ciphertext.len()];
        let n = self.transport.read_message(self.nonce, ciphertext, &mut out).map_err(noise_err)?;
        self.nonce += 1;
        out.truncate(n);
        Ok(out)
    }

    /// Reads, decrypts and decodes one message.
    pub fn recv<R: Read>(&mut self, r: &mut R) -> io::Result<Message> {
        let frame = read_frame(r)?;
        let plain = self.open(&frame)?;
        Message::decode(&plain).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    fn pair() -> (Handshake, Handshake, PeerId, PeerId) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let a = Identity::generate().unwrap();
        let b = Identity::generate().unwrap();
        let (a_id, b_id) = (a.id(), b.id());
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            respond(&mut s, &b).unwrap()
        });
        let mut c = TcpStream::connect(addr).unwrap();
        let ha = initiate(&mut c, &a).unwrap();
        let hb = server.join().unwrap();
        (ha, hb, a_id, b_id)
    }

    #[test]
    fn handshake_reveals_identities_and_matching_hashes() {
        let (ha, hb, a_id, b_id) = pair();
        assert_eq!(ha.remote, b_id);
        assert_eq!(hb.remote, a_id);
        assert_eq!(ha.hash, hb.hash);
    }

    #[test]
    fn messages_decrypt_in_order_and_tampering_is_detected() {
        let (mut ha, mut hb, _, _) = pair();
        let one = ha.sealer.seal(b"one").unwrap();
        let two = ha.sealer.seal(b"two").unwrap();
        assert_eq!(hb.opener.open(&one).unwrap(), b"one");
        let mut bad = two.clone();
        bad[0] ^= 1;
        assert!(hb.opener.open(&bad).is_err());

        // Replaying an earlier frame fails too: nonces never repeat.
        let (mut ha, mut hb, _, _) = pair();
        let one = ha.sealer.seal(b"one").unwrap();
        hb.opener.open(&one).unwrap();
        assert!(hb.opener.open(&one).is_err());
    }

    #[test]
    fn rejects_a_different_prologue_or_garbage() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            respond(&mut s, &Identity::generate().unwrap()).is_err()
        });
        let mut c = TcpStream::connect(addr).unwrap();
        write_frame(&mut c, &[0u8; 10]).unwrap();
        drop(c);
        assert!(server.join().unwrap());
    }
}
