//! First-time pairing with a six-digit code.
//!
//! The device being controlled shows a random code on its own screen; the
//! person types it on the controlling device. The code is never sent. Both
//! sides run SPAKE2 with it inside the already-encrypted Noise channel, then
//! prove they derived the same key by exchanging MACs over the Noise handshake
//! hash and both public keys:
//!
//! * a passive observer learns nothing;
//! * a machine-in-the-middle sees two different handshake hashes, so the
//!   confirmations fail even if it relays every SPAKE2 message;
//! * an active attacker gets one guess per attempt (1 in 1,000,000) and the
//!   responder locks pairing after repeated failures.

use blake2::digest::{KeyInit, Mac};
use blake2::Blake2sMac256;
use spake2::{Ed25519Group, Identity as SpakeIdentity, Password, Spake2};
use std::time::{Duration, Instant};

use crate::identity::PeerId;

pub const CODE_DIGITS: usize = 6;
const ID_INITIATOR: &[u8] = b"edgeway/1 pairing initiator";
const ID_RESPONDER: &[u8] = b"edgeway/1 pairing responder";
const LABEL_INITIATOR: &[u8] = b"edgeway/1 confirm initiator";
const LABEL_RESPONDER: &[u8] = b"edgeway/1 confirm responder";

/// A uniformly random six-digit code, e.g. `"048213"`.
pub fn generate_code() -> String {
    loop {
        let mut b = [0u8; 4];
        getrandom::getrandom(&mut b).expect("system random source");
        let v = u32::from_le_bytes(b);
        // Rejection sampling keeps every code equally likely.
        if v < u32::MAX - (u32::MAX % 1_000_000) {
            return format!("{:06}", v % 1_000_000);
        }
    }
}

/// Keeps only the digits a person typed (spaces, dashes and so on are ignored).
pub fn normalize_code(input: &str) -> Option<String> {
    let digits: String = input.chars().filter(|c| c.is_ascii_digit()).collect();
    (digits.len() == CODE_DIGITS).then_some(digits)
}

fn spake_ids() -> (SpakeIdentity, SpakeIdentity) {
    (SpakeIdentity::new(ID_INITIATOR), SpakeIdentity::new(ID_RESPONDER))
}

/// The SPAKE2 state of one side.
pub struct PakeState(Spake2<Ed25519Group>);

/// Responder: begins SPAKE2 with the code it is displaying.
pub fn responder_start(code: &str) -> (PakeState, Vec<u8>) {
    let (a, b) = spake_ids();
    let (state, msg) = Spake2::<Ed25519Group>::start_b(&Password::new(code.as_bytes()), &a, &b);
    (PakeState(state), msg)
}

/// Initiator: begins SPAKE2 with the code the person typed.
pub fn initiator_start(code: &str) -> (PakeState, Vec<u8>) {
    let (a, b) = spake_ids();
    let (state, msg) = Spake2::<Ed25519Group>::start_a(&Password::new(code.as_bytes()), &a, &b);
    (PakeState(state), msg)
}

impl PakeState {
    /// Derives the shared key from the other side's message. A wrong code still
    /// produces *a* key; it simply won't match, which confirmation detects.
    pub fn finish(self, peer_msg: &[u8]) -> Option<Vec<u8>> {
        self.0.finish(peer_msg).ok()
    }
}

/// Which side a confirmation tag comes from.
#[derive(Clone, Copy)]
pub enum Side {
    Initiator,
    Responder,
}

fn tag_mac(key: &[u8], side: Side, hash: &[u8; 32], initiator: &PeerId, responder: &PeerId) -> Blake2sMac256 {
    let mut mac = <Blake2sMac256 as KeyInit>::new_from_slice(key).expect("SPAKE2 key is 32 bytes");
    mac.update(match side {
        Side::Initiator => LABEL_INITIATOR,
        Side::Responder => LABEL_RESPONDER,
    });
    mac.update(hash);
    mac.update(&initiator.0);
    mac.update(&responder.0);
    mac
}

/// Key confirmation bound to this exact Noise session and both identities.
pub fn confirm_tag(key: &[u8], side: Side, hash: &[u8; 32], initiator: &PeerId, responder: &PeerId) -> [u8; 32] {
    tag_mac(key, side, hash, initiator, responder).finalize().into_bytes().into()
}

/// Constant-time check of the other side's confirmation.
pub fn verify_tag(
    key: &[u8],
    side: Side,
    hash: &[u8; 32],
    initiator: &PeerId,
    responder: &PeerId,
    tag: &[u8; 32],
) -> bool {
    tag_mac(key, side, hash, initiator, responder).verify_slice(tag).is_ok()
}

/// Limits wrong-code attempts on the receiving device.
pub struct Throttle {
    failures: Vec<Instant>,
}

impl Throttle {
    const WINDOW: Duration = Duration::from_secs(10 * 60);
    const MAX_FAILURES: usize = 5;

    pub fn new() -> Self {
        Throttle { failures: Vec::new() }
    }

    pub fn allowed(&mut self, now: Instant) -> bool {
        self.failures.retain(|t| now.duration_since(*t) < Self::WINDOW);
        self.failures.len() < Self::MAX_FAILURES
    }

    pub fn record_failure(&mut self, now: Instant) {
        self.failures.push(now);
    }

    pub fn reset(&mut self) {
        self.failures.clear();
    }
}

impl Default for Throttle {
    fn default() -> Self {
        Throttle::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Identity;

    fn run(code_shown: &str, code_typed: &str, hash_i: [u8; 32], hash_r: [u8; 32]) -> bool {
        let i = Identity::generate().unwrap().id();
        let r = Identity::generate().unwrap().id();
        let (rs, msg_b) = responder_start(code_shown);
        let (is, msg_a) = initiator_start(code_typed);
        let key_i = is.finish(&msg_b).unwrap();
        let key_r = rs.finish(&msg_a).unwrap();
        let tag_i = confirm_tag(&key_i, Side::Initiator, &hash_i, &i, &r);
        verify_tag(&key_r, Side::Initiator, &hash_r, &i, &r, &tag_i)
    }

    #[test]
    fn matching_code_and_session_confirms() {
        assert!(run("123456", "123456", [1; 32], [1; 32]));
    }

    #[test]
    fn wrong_code_fails() {
        assert!(!run("123456", "123457", [1; 32], [1; 32]));
    }

    #[test]
    fn relayed_messages_across_two_sessions_fail() {
        // A machine-in-the-middle terminates two Noise sessions with different
        // hashes; even with the right code the confirmation does not verify.
        assert!(!run("123456", "123456", [1; 32], [2; 32]));
    }

    #[test]
    fn codes_are_six_digits_and_normalised() {
        for _ in 0..100 {
            let c = generate_code();
            assert_eq!(c.len(), 6);
            assert!(c.chars().all(|ch| ch.is_ascii_digit()));
        }
        assert_eq!(normalize_code("048 213").as_deref(), Some("048213"));
        assert_eq!(normalize_code("12-34"), None);
    }

    #[test]
    fn throttle_blocks_after_repeated_failures() {
        let mut t = Throttle::new();
        let now = Instant::now();
        for _ in 0..5 {
            assert!(t.allowed(now));
            t.record_failure(now);
        }
        assert!(!t.allowed(now));
        assert!(t.allowed(now + Duration::from_secs(601)));
    }
}
