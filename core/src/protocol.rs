//! Messages exchanged inside the encrypted channel.
//!
//! Every message is one Noise transport frame whose plaintext starts with a
//! one-byte tag. Unknown tags decode to [`Message::Unknown`] and are ignored, so
//! new message types can be added without breaking older peers. The full
//! byte-level layout is documented in `docs/PROTOCOL.md`.

use crate::wire::{DecodeError, DecodeResult, Reader, Writer};

/// Bumped only for incompatible changes. Peers refuse a different version.
pub const PROTOCOL_VERSION: u16 = 1;
/// Noise protocol name. The prologue below binds every handshake to Edgeway v1.
pub const NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";
pub const NOISE_PROLOGUE: &[u8] = b"edgeway/1";
/// DNS-SD service type advertised by devices that accept control.
pub const SERVICE_TYPE: &str = "_edgeway._tcp";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Unknown,
    MacOS,
    Windows,
    Linux,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Platform::MacOS
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "linux") {
            Platform::Linux
        } else {
            Platform::Unknown
        }
    }

    pub fn code(self) -> u8 {
        match self {
            Platform::Unknown => 0,
            Platform::MacOS => 1,
            Platform::Windows => 2,
            Platform::Linux => 3,
        }
    }

    pub fn from_code(code: u8) -> Self {
        match code {
            1 => Platform::MacOS,
            2 => Platform::Windows,
            3 => Platform::Linux,
            _ => Platform::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Platform::Unknown => "unknown",
            Platform::MacOS => "macos",
            Platform::Windows => "windows",
            Platform::Linux => "linux",
        }
    }
}

/// One display, in the device's own global coordinate space (points, origin
/// at the top-left of the main display, y grows downward).
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hello {
    pub version: u16,
    pub name: String,
    pub platform: Platform,
    pub screens: Vec<Screen>,
}

/// Why a pairing attempt was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairReject {
    Denied,
    WrongCode,
    Busy,
    TooManyAttempts,
    Timeout,
}

impl PairReject {
    fn code(self) -> u8 {
        match self {
            PairReject::Denied => 1,
            PairReject::WrongCode => 2,
            PairReject::Busy => 3,
            PairReject::TooManyAttempts => 4,
            PairReject::Timeout => 5,
        }
    }

    fn from_code(code: u8) -> Self {
        match code {
            2 => PairReject::WrongCode,
            3 => PairReject::Busy,
            4 => PairReject::TooManyAttempts,
            5 => PairReject::Timeout,
            _ => PairReject::Denied,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            PairReject::Denied => "The other device declined the request.",
            PairReject::WrongCode => "The code didn’t match. Try again.",
            PairReject::Busy => "The other device is already pairing with someone else.",
            PairReject::TooManyAttempts => "Too many wrong codes. Wait a few minutes and try again.",
            PairReject::Timeout => "Pairing timed out.",
        }
    }
}

/// Mouse buttons in a platform-neutral order.
pub mod button {
    pub const LEFT: u8 = 0;
    pub const RIGHT: u8 = 1;
    pub const MIDDLE: u8 = 2;
    pub const BACK: u8 = 3;
    pub const FORWARD: u8 = 4;
    /// Buttons above this are rejected.
    pub const MAX: u8 = 31;
}

/// Scroll gesture phase, mirroring trackpad phases so momentum and rubber-band
/// scrolling feel native on the receiving side.
pub mod phase {
    pub const NONE: u8 = 0;
    pub const BEGAN: u8 = 1;
    pub const CHANGED: u8 = 2;
    pub const ENDED: u8 = 3;
    pub const CANCELLED: u8 = 4;
    pub const MAY_BEGIN: u8 = 5;
}

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    /// First message after the handshake (and after pairing), both directions.
    Hello(Hello),
    /// A device's displays changed.
    Screens(Vec<Screen>),

    /// Initiator → responder: please pair with me.
    PairRequest {
        name: String,
        platform: Platform,
    },
    /// Responder → initiator: a code is on my screen; here is my SPAKE2 share.
    PairChallenge {
        spake: Vec<u8>,
    },
    /// Initiator → responder: my SPAKE2 share and key confirmation.
    PairResponse {
        spake: Vec<u8>,
        confirm: [u8; 32],
    },
    /// Responder → initiator: my key confirmation. Pairing is complete.
    PairConfirm {
        confirm: [u8; 32],
    },
    PairRejected(PairReject),

    /// Control moves to the receiver; place the pointer here (receiver coords).
    Enter {
        x: f32,
        y: f32,
    },
    /// Control leaves the receiver.
    Leave,
    /// Absolute pointer position plus the raw movement that produced it.
    MouseMove {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
    MouseButton {
        button: u8,
        down: bool,
        clicks: u8,
    },
    /// `dx`/`dy` in pixels; `lines_*` for classic wheel mice.
    Scroll {
        dx: f32,
        dy: f32,
        lines_x: i16,
        lines_y: i16,
        phase: u8,
        momentum: u8,
        continuous: bool,
    },
    /// `key` is a USB HID usage (keyboard page) or an Edgeway extension code.
    Key {
        key: u16,
        down: bool,
        repeat: bool,
    },
    /// Release every key and button the receiver is holding.
    ReleaseAll,

    Ping {
        t: u64,
    },
    Pong {
        t: u64,
    },
    Bye,

    Unknown(u8),
}

mod tag {
    pub const HELLO: u8 = 0x01;
    pub const SCREENS: u8 = 0x02;
    pub const PAIR_REQUEST: u8 = 0x08;
    pub const PAIR_CHALLENGE: u8 = 0x09;
    pub const PAIR_RESPONSE: u8 = 0x0A;
    pub const PAIR_CONFIRM: u8 = 0x0B;
    pub const PAIR_REJECTED: u8 = 0x0C;
    pub const ENTER: u8 = 0x10;
    pub const LEAVE: u8 = 0x11;
    pub const MOUSE_MOVE: u8 = 0x12;
    pub const MOUSE_BUTTON: u8 = 0x13;
    pub const SCROLL: u8 = 0x14;
    pub const KEY: u8 = 0x15;
    pub const RELEASE_ALL: u8 = 0x16;
    pub const PING: u8 = 0x20;
    pub const PONG: u8 = 0x21;
    pub const BYE: u8 = 0x2F;
}

/// At most this many screens are accepted from a peer.
const MAX_SCREENS: usize = 16;

fn write_screens(w: &mut Writer, screens: &[Screen]) {
    let n = screens.len().min(MAX_SCREENS);
    w.u8(n as u8);
    for s in &screens[..n] {
        w.f32(s.x).f32(s.y).f32(s.w).f32(s.h).str(&s.name);
    }
}

fn read_screens(r: &mut Reader) -> DecodeResult<Vec<Screen>> {
    let n = r.u8()? as usize;
    if n > MAX_SCREENS {
        return Err(DecodeError);
    }
    let mut screens = Vec::with_capacity(n);
    for _ in 0..n {
        let s = Screen { x: r.f32()?, y: r.f32()?, w: r.f32()?, h: r.f32()?, name: r.str()? };
        if s.w < 1.0 || s.h < 1.0 || s.w > 100_000.0 || s.h > 100_000.0 {
            return Err(DecodeError);
        }
        screens.push(s);
    }
    Ok(screens)
}

impl Message {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Message::Hello(h) => {
                let mut w = Writer::with_tag(tag::HELLO);
                w.u16(h.version).str(&h.name).u8(h.platform.code());
                write_screens(&mut w, &h.screens);
                w.finish()
            }
            Message::Screens(s) => {
                let mut w = Writer::with_tag(tag::SCREENS);
                write_screens(&mut w, s);
                w.finish()
            }
            Message::PairRequest { name, platform } => {
                Writer::with_tag(tag::PAIR_REQUEST).str(name).u8(platform.code()).finish()
            }
            Message::PairChallenge { spake } => Writer::with_tag(tag::PAIR_CHALLENGE).bytes(spake).finish(),
            Message::PairResponse { spake, confirm } => {
                Writer::with_tag(tag::PAIR_RESPONSE).bytes(spake).array32(confirm).finish()
            }
            Message::PairConfirm { confirm } => Writer::with_tag(tag::PAIR_CONFIRM).array32(confirm).finish(),
            Message::PairRejected(reason) => Writer::with_tag(tag::PAIR_REJECTED).u8(reason.code()).finish(),
            Message::Enter { x, y } => Writer::with_tag(tag::ENTER).f32(*x).f32(*y).finish(),
            Message::Leave => vec![tag::LEAVE],
            Message::MouseMove { x, y, dx, dy } => {
                Writer::with_tag(tag::MOUSE_MOVE).f32(*x).f32(*y).f32(*dx).f32(*dy).finish()
            }
            Message::MouseButton { button, down, clicks } => {
                Writer::with_tag(tag::MOUSE_BUTTON).u8(*button).bool(*down).u8(*clicks).finish()
            }
            Message::Scroll { dx, dy, lines_x, lines_y, phase, momentum, continuous } => Writer::with_tag(tag::SCROLL)
                .f32(*dx)
                .f32(*dy)
                .i16(*lines_x)
                .i16(*lines_y)
                .u8(*phase)
                .u8(*momentum)
                .bool(*continuous)
                .finish(),
            Message::Key { key, down, repeat } => {
                Writer::with_tag(tag::KEY).u16(*key).bool(*down).bool(*repeat).finish()
            }
            Message::ReleaseAll => vec![tag::RELEASE_ALL],
            Message::Ping { t } => Writer::with_tag(tag::PING).u64(*t).finish(),
            Message::Pong { t } => Writer::with_tag(tag::PONG).u64(*t).finish(),
            Message::Bye => vec![tag::BYE],
            Message::Unknown(t) => vec![*t],
        }
    }

    pub fn decode(bytes: &[u8]) -> DecodeResult<Message> {
        let mut r = Reader::new(bytes);
        let t = r.u8()?;
        Ok(match t {
            tag::HELLO => Message::Hello(Hello {
                version: r.u16()?,
                name: r.str()?,
                platform: Platform::from_code(r.u8()?),
                screens: read_screens(&mut r)?,
            }),
            tag::SCREENS => Message::Screens(read_screens(&mut r)?),
            tag::PAIR_REQUEST => Message::PairRequest { name: r.str()?, platform: Platform::from_code(r.u8()?) },
            tag::PAIR_CHALLENGE => Message::PairChallenge { spake: r.bytes()? },
            tag::PAIR_RESPONSE => Message::PairResponse { spake: r.bytes()?, confirm: r.array32()? },
            tag::PAIR_CONFIRM => Message::PairConfirm { confirm: r.array32()? },
            tag::PAIR_REJECTED => Message::PairRejected(PairReject::from_code(r.u8()?)),
            tag::ENTER => Message::Enter { x: r.f32()?, y: r.f32()? },
            tag::LEAVE => Message::Leave,
            tag::MOUSE_MOVE => Message::MouseMove { x: r.f32()?, y: r.f32()?, dx: r.f32()?, dy: r.f32()? },
            tag::MOUSE_BUTTON => {
                let button = r.u8()?;
                if button > button::MAX {
                    return Err(DecodeError);
                }
                Message::MouseButton { button, down: r.bool()?, clicks: r.u8()? }
            }
            tag::SCROLL => Message::Scroll {
                dx: r.f32()?,
                dy: r.f32()?,
                lines_x: r.i16()?,
                lines_y: r.i16()?,
                phase: r.u8()?,
                momentum: r.u8()?,
                continuous: r.bool()?,
            },
            tag::KEY => Message::Key { key: r.u16()?, down: r.bool()?, repeat: r.bool()? },
            tag::RELEASE_ALL => Message::ReleaseAll,
            tag::PING => Message::Ping { t: r.u64()? },
            tag::PONG => Message::Pong { t: r.u64()? },
            tag::BYE => Message::Bye,
            other => Message::Unknown(other),
        })
    }

    /// Short name for logs; never includes key codes or positions.
    pub fn kind(&self) -> &'static str {
        match self {
            Message::Hello(_) => "hello",
            Message::Screens(_) => "screens",
            Message::PairRequest { .. } => "pair-request",
            Message::PairChallenge { .. } => "pair-challenge",
            Message::PairResponse { .. } => "pair-response",
            Message::PairConfirm { .. } => "pair-confirm",
            Message::PairRejected(_) => "pair-rejected",
            Message::Enter { .. } => "enter",
            Message::Leave => "leave",
            Message::MouseMove { .. } => "mouse-move",
            Message::MouseButton { .. } => "mouse-button",
            Message::Scroll { .. } => "scroll",
            Message::Key { .. } => "key",
            Message::ReleaseAll => "release-all",
            Message::Ping { .. } => "ping",
            Message::Pong { .. } => "pong",
            Message::Bye => "bye",
            Message::Unknown(_) => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples() -> Vec<Message> {
        let screen = Screen { x: 0.0, y: -1080.0, w: 1920.0, h: 1080.0, name: "Studio Display".into() };
        vec![
            Message::Hello(Hello {
                version: PROTOCOL_VERSION,
                name: "Mac mini".into(),
                platform: Platform::MacOS,
                screens: vec![screen.clone()],
            }),
            Message::Screens(vec![screen.clone(), screen]),
            Message::PairRequest { name: "MacBook Pro".into(), platform: Platform::MacOS },
            Message::PairChallenge { spake: vec![1, 2, 3] },
            Message::PairResponse { spake: vec![4; 33], confirm: [7; 32] },
            Message::PairConfirm { confirm: [8; 32] },
            Message::PairRejected(PairReject::WrongCode),
            Message::Enter { x: 10.0, y: 20.5 },
            Message::Leave,
            Message::MouseMove { x: 1.0, y: 2.0, dx: -0.5, dy: 3.25 },
            Message::MouseButton { button: button::RIGHT, down: true, clicks: 2 },
            Message::Scroll {
                dx: 0.0,
                dy: -12.5,
                lines_x: 0,
                lines_y: -1,
                phase: phase::CHANGED,
                momentum: phase::NONE,
                continuous: true,
            },
            Message::Key { key: 0x04, down: true, repeat: false },
            Message::ReleaseAll,
            Message::Ping { t: 42 },
            Message::Pong { t: 42 },
            Message::Bye,
        ]
    }

    #[test]
    fn every_message_round_trips() {
        for m in samples() {
            assert_eq!(Message::decode(&m.encode()).unwrap(), m, "{}", m.kind());
        }
    }

    #[test]
    fn unknown_tags_are_tolerated() {
        assert_eq!(Message::decode(&[0xEE, 1, 2, 3]).unwrap(), Message::Unknown(0xEE));
    }

    #[test]
    fn truncated_messages_are_rejected() {
        for m in samples() {
            let bytes = m.encode();
            if bytes.len() > 1 {
                assert!(Message::decode(&bytes[..bytes.len() - 1]).is_err(), "{}", m.kind());
            }
        }
    }

    #[test]
    fn rejects_absurd_screens_and_buttons() {
        let bad = Message::Screens(vec![Screen { x: 0.0, y: 0.0, w: 0.0, h: 10.0, name: String::new() }]);
        assert!(Message::decode(&bad.encode()).is_err());
        assert!(Message::decode(&[tag::MOUSE_BUTTON, 200, 1, 1]).is_err());
    }
}
