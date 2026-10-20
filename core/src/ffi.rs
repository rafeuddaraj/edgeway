//! C ABI for the native shells. The matching header is `include/edgeway.h`.
//!
//! Rules for callers:
//! * Every `EwEngine*` comes from [`ew_engine_start`] and is released exactly
//!   once with [`ew_engine_stop`].
//! * Strings passed in are UTF-8 and only borrowed for the call.
//! * Strings returned by [`ew_engine_snapshot`] are freed with [`ew_string_free`].
//! * Callbacks run on engine threads; the shell hops to its UI thread itself.

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;
use std::sync::Arc;

use nanoserde::SerJson;

use crate::engine::{Control, Engine, EngineOptions, Host, PairingUi, Role};
use crate::identity::PeerId;
use crate::injector::Output;
use crate::keymap;
use crate::protocol::Screen;
use crate::switching::{HomeReason, LocalEvent, Verdict};

pub const EW_EVENT_MOUSE_MOVE: u8 = 1;
pub const EW_EVENT_BUTTON: u8 = 2;
pub const EW_EVENT_SCROLL: u8 = 3;
pub const EW_EVENT_KEY: u8 = 4;
pub const EW_EVENT_OTHER: u8 = 5;
pub const EW_EVENT_WAKE: u8 = 6;

pub const EW_CONTROL_FREEZE: u8 = 1;
pub const EW_CONTROL_UNFREEZE: u8 = 2;

pub const EW_PAIRING_SHOW_CODE: u8 = 1;
pub const EW_PAIRING_ENTER_CODE: u8 = 2;
pub const EW_PAIRING_SUCCEEDED: u8 = 3;
pub const EW_PAIRING_FAILED: u8 = 4;
pub const EW_PAIRING_CLOSED: u8 = 5;

/// One input event in either direction. Unused fields are zero.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct EwEvent {
    pub kind: u8,
    pub button: u8,
    pub down: u8,
    pub clicks: u8,
    pub repeat: u8,
    pub phase: u8,
    pub momentum: u8,
    pub continuous: u8,
    /// HID usage (see `keymap`).
    pub key: u16,
    pub lines_x: i16,
    pub lines_y: i16,
    pub reserved: u16,
    /// Neutral modifier bits (`EW_MOD_*`).
    pub mods: u32,
    /// Held mouse buttons, bit per button (injection only).
    pub buttons: u32,
    pub x: f64,
    pub y: f64,
    pub dx: f64,
    pub dy: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EwScreen {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub name: *const c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EwCallbacks {
    pub context: *mut c_void,
    pub inject: Option<extern "C" fn(*mut c_void, *const EwEvent)>,
    pub control: Option<extern "C" fn(*mut c_void, u8, f64, f64)>,
    pub state_changed: Option<extern "C" fn(*mut c_void)>,
    pub pairing: Option<extern "C" fn(*mut c_void, u8, *const c_char, *const c_char)>,
}

struct FfiHost(EwCallbacks);

// The shell guarantees its callbacks and context are thread-safe.
unsafe impl Send for FfiHost {}
unsafe impl Sync for FfiHost {}

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

impl Host for FfiHost {
    fn inject(&self, out: &Output) {
        let Some(f) = self.0.inject else { return };
        let mut ev = EwEvent::default();
        match *out {
            Output::MouseMove { x, y, dx, dy, buttons, mods } => {
                ev.kind = EW_EVENT_MOUSE_MOVE;
                (ev.x, ev.y, ev.dx, ev.dy, ev.buttons, ev.mods) =
                    (x as f64, y as f64, dx as f64, dy as f64, buttons, mods);
            }
            Output::Button { button, down, clicks, x, y, mods } => {
                ev.kind = EW_EVENT_BUTTON;
                (ev.button, ev.down, ev.clicks, ev.x, ev.y, ev.mods) =
                    (button, down as u8, clicks, x as f64, y as f64, mods);
            }
            Output::Scroll { dx, dy, lines_x, lines_y, phase, momentum, continuous, mods } => {
                ev.kind = EW_EVENT_SCROLL;
                (ev.dx, ev.dy, ev.lines_x, ev.lines_y) = (dx as f64, dy as f64, lines_x, lines_y);
                (ev.phase, ev.momentum, ev.continuous, ev.mods) = (phase, momentum, continuous as u8, mods);
            }
            Output::Key { key, down, repeat, mods } => {
                ev.kind = EW_EVENT_KEY;
                (ev.key, ev.down, ev.repeat, ev.mods) = (key, down as u8, repeat as u8, mods);
            }
            Output::Wake => ev.kind = EW_EVENT_WAKE,
        }
        f(self.0.context, &ev);
    }

    fn control(&self, c: Control) {
        let Some(f) = self.0.control else { return };
        match c {
            Control::Freeze { x, y } => f(self.0.context, EW_CONTROL_FREEZE, x, y),
            Control::Unfreeze { x, y } => f(self.0.context, EW_CONTROL_UNFREEZE, x, y),
        }
    }

    fn state_changed(&self) {
        if let Some(f) = self.0.state_changed {
            f(self.0.context);
        }
    }

    fn pairing(&self, ev: PairingUi) {
        let Some(f) = self.0.pairing else { return };
        let (kind, peer, detail) = match ev {
            PairingUi::ShowCode { peer, code } => (EW_PAIRING_SHOW_CODE, peer, code),
            PairingUi::EnterCode { peer } => (EW_PAIRING_ENTER_CODE, peer, String::new()),
            PairingUi::Succeeded { peer } => (EW_PAIRING_SUCCEEDED, peer, String::new()),
            PairingUi::Failed { peer, reason } => (EW_PAIRING_FAILED, peer, reason),
            PairingUi::Closed => (EW_PAIRING_CLOSED, String::new(), String::new()),
        };
        let (peer, detail) = (cstring(&peer), cstring(&detail));
        f(self.0.context, kind, peer.as_ptr(), detail.as_ptr());
    }
}

/// Opaque engine handle.
pub struct EwEngine {
    engine: Engine,
}

unsafe fn str_arg<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        ""
    } else {
        CStr::from_ptr(p).to_str().unwrap_or("")
    }
}

unsafe fn screens_arg(screens: *const EwScreen, count: usize) -> Vec<Screen> {
    if screens.is_null() {
        return Vec::new();
    }
    std::slice::from_raw_parts(screens, count.min(16))
        .iter()
        .filter(|s| s.w >= 1.0 && s.h >= 1.0)
        .map(|s| Screen {
            x: s.x as f32,
            y: s.y as f32,
            w: s.w as f32,
            h: s.h as f32,
            name: str_arg(s.name).to_string(),
        })
        .collect()
}

/// Starts the engine. `role`: 1 = primary, 2 = secondary. Returns NULL on
/// failure (for example an unreadable configuration directory).
///
/// # Safety
/// Pointers must be valid for the duration of the call; `callbacks` must stay
/// valid until [`ew_engine_stop`].
#[no_mangle]
pub unsafe extern "C" fn ew_engine_start(
    dir: *const c_char,
    role: u8,
    name: *const c_char,
    screens: *const EwScreen,
    screen_count: usize,
    callbacks: EwCallbacks,
) -> *mut EwEngine {
    let role = if role == 2 { Role::Secondary } else { Role::Primary };
    let mut opts = EngineOptions::new(PathBuf::from(str_arg(dir)), role, str_arg(name).to_string());
    opts.screens = screens_arg(screens, screen_count);
    #[cfg(debug_assertions)]
    {
        opts.fixed_pairing_code = std::env::var("EDGEWAY_DEV_PAIRING_CODE").ok();
    }
    match Engine::start(opts, Arc::new(FfiHost(callbacks))) {
        Ok(engine) => Box::into_raw(Box::new(EwEngine { engine })),
        Err(e) => {
            ew_log!("engine failed to start: {e}");
            std::ptr::null_mut()
        }
    }
}

/// Stops and frees the engine. Safe to call with NULL.
///
/// # Safety
/// `engine` must come from [`ew_engine_start`] and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn ew_engine_stop(engine: *mut EwEngine) {
    if !engine.is_null() {
        let e = Box::from_raw(engine);
        e.engine.stop();
    }
}

/// # Safety
/// `engine` must be live; `screens` must point to `count` entries.
#[no_mangle]
pub unsafe extern "C" fn ew_engine_set_screens(engine: *const EwEngine, screens: *const EwScreen, count: usize) {
    if let Some(e) = engine.as_ref() {
        e.engine.set_screens(screens_arg(screens, count));
    }
}

/// Primary hot path. Returns 1 if the event must be swallowed locally.
///
/// # Safety
/// `engine` and `event` must be valid.
#[no_mangle]
pub unsafe extern "C" fn ew_engine_local_event(engine: *const EwEngine, event: *const EwEvent) -> u8 {
    let (Some(e), Some(ev)) = (engine.as_ref(), event.as_ref()) else { return 0 };
    let local = match ev.kind {
        EW_EVENT_MOUSE_MOVE => LocalEvent::MouseMove { x: ev.x, y: ev.y, dx: ev.dx, dy: ev.dy },
        EW_EVENT_BUTTON => LocalEvent::Button { button: ev.button.min(31), down: ev.down != 0, clicks: ev.clicks },
        EW_EVENT_SCROLL => LocalEvent::Scroll {
            dx: ev.dx,
            dy: ev.dy,
            lines_x: ev.lines_x,
            lines_y: ev.lines_y,
            phase: ev.phase,
            momentum: ev.momentum,
            continuous: ev.continuous != 0,
        },
        EW_EVENT_KEY => {
            if ev.key == 0 {
                LocalEvent::Other
            } else {
                LocalEvent::Key { key: ev.key, down: ev.down != 0, repeat: ev.repeat != 0 }
            }
        }
        _ => LocalEvent::Other,
    };
    (e.engine.handle_local_event(&local) == Verdict::Swallow) as u8
}

/// Current state as JSON. Free with [`ew_string_free`].
///
/// # Safety
/// `engine` must be live.
#[no_mangle]
pub unsafe extern "C" fn ew_engine_snapshot(engine: *const EwEngine) -> *mut c_char {
    match engine.as_ref() {
        Some(e) => cstring(&e.engine.snapshot().serialize_json()).into_raw(),
        None => std::ptr::null_mut(),
    }
}

/// # Safety
/// `s` must come from this library, or be NULL.
#[no_mangle]
pub unsafe extern "C" fn ew_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// Runs a command. Returns 1 on success.
///
/// | verb               | arg                         |
/// |--------------------|-----------------------------|
/// | `pair`             | DNS-SD instance name        |
/// | `code`             | six-digit code              |
/// | `cancel-pairing`   | —                           |
/// | `forget`           | device id (hex)             |
/// | `place`            | `<id hex> <x> <y>`          |
/// | `pause` / `resume` | —                           |
/// | `home`             | `secure-input` or empty     |
/// | `resistance`       | points                      |
/// | `network-changed`  | —                           |
///
/// # Safety
/// `engine` must be live; strings must be valid or NULL.
#[no_mangle]
pub unsafe extern "C" fn ew_engine_command(engine: *const EwEngine, verb: *const c_char, arg: *const c_char) -> u8 {
    let Some(e) = engine.as_ref() else { return 0 };
    let e = &e.engine;
    let arg = str_arg(arg).trim();
    let ok = match str_arg(verb) {
        "pair" => e.pair(arg),
        "code" => e.submit_pairing_code(arg),
        "cancel-pairing" => {
            e.cancel_pairing();
            true
        }
        "forget" => PeerId::from_hex(arg).map(|id| e.forget(&id)).is_some(),
        "place" => {
            let parts: Vec<&str> = arg.split_whitespace().collect();
            match (parts.first().and_then(|h| PeerId::from_hex(h)), parts.get(1), parts.get(2)) {
                (Some(id), Some(x), Some(y)) => match (x.parse::<f32>(), y.parse::<f32>()) {
                    (Ok(x), Ok(y)) if x.is_finite() && y.is_finite() => {
                        e.set_placement(&id, x, y);
                        true
                    }
                    _ => false,
                },
                _ => false,
            }
        }
        "pause" => {
            e.set_paused(true);
            true
        }
        "resume" => {
            e.set_paused(false);
            true
        }
        "home" => {
            e.go_home(if arg == "secure-input" { HomeReason::SecureInput } else { HomeReason::Requested });
            true
        }
        "resistance" => arg.parse::<f32>().map(|r| e.set_edge_resistance(r)).is_ok(),
        "network-changed" => {
            e.network_changed();
            true
        }
        _ => false,
    };
    ok as u8
}

/// macOS virtual key code → HID usage (0 if unknown).
#[no_mangle]
pub extern "C" fn ew_key_from_mac(kvk: u16) -> u16 {
    keymap::mac_to_hid(kvk)
}

/// HID usage → macOS virtual key code, or -1.
#[no_mangle]
pub extern "C" fn ew_key_to_mac(key: u16) -> i32 {
    keymap::hid_to_mac(key).map(i32::from).unwrap_or(-1)
}

/// `CGEventFlags` bits for a modifier key (0 for other keys).
#[no_mangle]
pub extern "C" fn ew_mac_flags_for_key(key: u16) -> u64 {
    keymap::mac_flags_for_key(key)
}

/// `CGEventFlags` for neutral modifier bits.
#[no_mangle]
pub extern "C" fn ew_mac_flags_from_mods(mods: u32) -> u64 {
    let mut f = 0u64;
    if mods & keymap::mods::SHIFT != 0 {
        f |= 0x0002_0000;
    }
    if mods & keymap::mods::CONTROL != 0 {
        f |= 0x0004_0000;
    }
    if mods & keymap::mods::ALT != 0 {
        f |= 0x0008_0000;
    }
    if mods & keymap::mods::META != 0 {
        f |= 0x0010_0000;
    }
    if mods & keymap::mods::CAPS_LOCK != 0 {
        f |= keymap::MAC_FLAG_CAPS_LOCK;
    }
    if mods & keymap::mods::FN != 0 {
        f |= 0x0080_0000;
    }
    f
}

/// Library version, e.g. "0.1.0".
#[no_mangle]
pub extern "C" fn ew_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}
