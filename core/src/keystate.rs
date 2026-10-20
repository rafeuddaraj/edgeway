//! Key ownership on the controlling device.
//!
//! The rule that prevents stuck keys: **a key is released on the device that
//! saw it pressed.** If you hold ⌘ while the pointer crosses to the Mac mini,
//! the ⌘ release still goes to the MacBook, and vice versa. Auto-repeat
//! follows the same owner.

use std::collections::HashMap;

use crate::identity::PeerId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Local,
    Remote(PeerId),
}

/// Where a key event should go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Local,
    Remote(PeerId),
}

#[derive(Default)]
pub struct KeyRouter {
    down: HashMap<u16, Owner>,
}

impl KeyRouter {
    pub fn new() -> Self {
        KeyRouter::default()
    }

    /// Routes a key event given where input currently goes (`None` = local).
    pub fn route(&mut self, key: u16, down: bool, repeat: bool, focus: Option<PeerId>) -> Route {
        let current = match focus {
            Some(p) => Owner::Remote(p),
            None => Owner::Local,
        };
        let owner = if down {
            if repeat {
                *self.down.entry(key).or_insert(current)
            } else {
                // A fresh press while the key is somehow still held elsewhere:
                // the new focus wins, and the old owner keeps nothing.
                self.down.insert(key, current);
                current
            }
        } else {
            // An unknown release (pressed before Edgeway started) stays local.
            self.down.remove(&key).unwrap_or(Owner::Local)
        };
        match owner {
            Owner::Local => Route::Local,
            Owner::Remote(p) => Route::Remote(p),
        }
    }

    /// Forgets every key owned by `peer` (it releases them on its own when the
    /// connection drops, or was sent `ReleaseAll`).
    pub fn drop_peer(&mut self, peer: &PeerId) {
        self.down.retain(|_, o| *o != Owner::Remote(*peer));
    }

    pub fn held_by(&self, owner: Owner) -> usize {
        self.down.values().filter(|o| **o == owner).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer() -> PeerId {
        PeerId([9; 32])
    }

    #[test]
    fn release_follows_the_press_across_a_switch() {
        let mut k = KeyRouter::new();
        // ⌘ pressed locally, then focus moves to the peer.
        assert_eq!(k.route(0xE3, true, false, None), Route::Local);
        assert_eq!(k.route(0x04, true, false, Some(peer())), Route::Remote(peer()));
        assert_eq!(k.route(0x04, true, true, Some(peer())), Route::Remote(peer()));
        assert_eq!(k.route(0xE3, false, false, Some(peer())), Route::Local);
        // Focus came back home while A was held on the peer.
        assert_eq!(k.route(0x04, false, false, None), Route::Remote(peer()));
        assert_eq!(k.held_by(Owner::Local) + k.held_by(Owner::Remote(peer())), 0);
    }

    #[test]
    fn dropped_peer_keys_are_forgotten() {
        let mut k = KeyRouter::new();
        k.route(0x04, true, false, Some(peer()));
        k.drop_peer(&peer());
        assert_eq!(k.route(0x04, false, false, None), Route::Local);
    }
}
