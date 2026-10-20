//! # Edgeway core
//!
//! The portable heart of Edgeway: discovery, pairing, the encrypted transport,
//! the wire protocol and the input-routing state machine. Platform shells
//! (macOS today; Windows and Linux next) only capture and inject input, draw the
//! user interface and call into this crate through the C ABI in [`ffi`].
//!
//! Module map:
//!
//! | Module        | Responsibility                                                   |
//! |---------------|------------------------------------------------------------------|
//! | [`identity`]  | Long-term X25519 device key and fingerprints                     |
//! | [`noise`]     | Noise XX handshake and framed, encrypted transport               |
//! | [`pairing`]   | Six-digit setup code verified with SPAKE2 and channel binding    |
//! | [`config`]    | Trusted devices, arrangement and settings on disk                |
//! | [`protocol`]  | Versioned binary messages exchanged after the handshake          |
//! | [`keymap`]    | USB HID usages as the neutral key code; platform tables          |
//! | [`switching`] | Screen arrangement, edge crossing and the virtual cursor         |
//! | [`keystate`]  | Key ownership so a key is always released where it was pressed   |
//! | [`injector`]  | Receiving side: turns messages into events and releases them     |
//! | [`discovery`] | DNS-SD (Bonjour) advertisement and browsing                      |
//! | [`engine`]    | Connection lifecycle, heartbeat, reconnection and public API     |
//! | [`ffi`]       | C ABI consumed by the native shells                              |

#[macro_use]
mod log;

pub mod config;
pub mod discovery;
pub mod engine;
pub mod ffi;
pub mod identity;
pub mod injector;
pub mod keymap;
pub mod keystate;
pub mod noise;
pub mod pairing;
pub mod protocol;
pub mod switching;
pub mod wire;

pub use engine::{Engine, EngineOptions, Host, Role};
