//! The receiving side: turns protocol messages into input events for the
//! platform shell to post, and remembers what is held so it can always be
//! released — on `ReleaseAll`, on disconnect, on heartbeat timeout or on pause.

use std::time::{Duration, Instant};

use crate::keymap::{self, hid, mods};
use crate::protocol::Message;

/// An event for the shell to post into the local session.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Output {
    /// Absolute position in this device's coordinates. `buttons` is the set of
    /// held buttons, so the shell can post drags instead of moves.
    MouseMove {
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
        buttons: u32,
        mods: u32,
    },
    Button {
        button: u8,
        down: bool,
        clicks: u8,
        x: f32,
        y: f32,
        mods: u32,
    },
    Scroll {
        dx: f32,
        dy: f32,
        lines_x: i16,
        lines_y: i16,
        phase: u8,
        momentum: u8,
        continuous: bool,
        mods: u32,
    },
    Key {
        key: u16,
        down: bool,
        repeat: bool,
        mods: u32,
    },
    /// Input is arriving: keep the display awake.
    Wake,
}

const WAKE_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct Injector {
    keys: Vec<u16>,
    buttons: u32,
    caps_lock: bool,
    x: f32,
    y: f32,
    last_wake: Option<Instant>,
}

impl Injector {
    pub fn new() -> Self {
        Injector::default()
    }

    pub fn mods(&self) -> u32 {
        let held = self.keys.iter().fold(0, |m, k| m | keymap::modifier_bit(*k));
        held | if self.caps_lock { mods::CAPS_LOCK } else { 0 }
    }

    pub fn holding_anything(&self) -> bool {
        !self.keys.is_empty() || self.buttons != 0
    }

    fn wake(&mut self, now: Instant, force: bool, out: &mut Vec<Output>) {
        if force || self.last_wake.is_none_or(|t| now.saturating_duration_since(t) >= WAKE_INTERVAL) {
            self.last_wake = Some(now);
            out.push(Output::Wake);
        }
    }

    pub fn apply(&mut self, msg: &Message, now: Instant, out: &mut Vec<Output>) {
        match *msg {
            Message::Enter { x, y } => {
                self.wake(now, true, out);
                self.x = x;
                self.y = y;
                out.push(Output::MouseMove { x, y, dx: 0.0, dy: 0.0, buttons: self.buttons, mods: self.mods() });
            }
            Message::Leave => {}
            Message::MouseMove { x, y, dx, dy } => {
                self.wake(now, false, out);
                self.x = x;
                self.y = y;
                out.push(Output::MouseMove { x, y, dx, dy, buttons: self.buttons, mods: self.mods() });
            }
            Message::MouseButton { button, down, clicks } => {
                self.wake(now, false, out);
                let bit = 1u32 << button.min(31);
                if !down && self.buttons & bit == 0 {
                    return;
                }
                if down {
                    self.buttons |= bit;
                } else {
                    self.buttons &= !bit;
                }
                out.push(Output::Button {
                    button,
                    down,
                    clicks: clicks.max(1),
                    x: self.x,
                    y: self.y,
                    mods: self.mods(),
                });
            }
            Message::Scroll { dx, dy, lines_x, lines_y, phase, momentum, continuous } => {
                self.wake(now, false, out);
                out.push(Output::Scroll { dx, dy, lines_x, lines_y, phase, momentum, continuous, mods: self.mods() });
            }
            Message::Key { key, down, repeat } => {
                self.wake(now, false, out);
                let held = self.keys.contains(&key);
                if down {
                    if !held {
                        self.keys.push(key);
                        if key == hid::CAPS_LOCK {
                            self.caps_lock = !self.caps_lock;
                        }
                    }
                    out.push(Output::Key { key, down: true, repeat: repeat && held, mods: self.mods() });
                } else if held {
                    self.keys.retain(|k| *k != key);
                    out.push(Output::Key { key, down: false, repeat: false, mods: self.mods() });
                }
            }
            Message::ReleaseAll => self.release_all(out),
            _ => {}
        }
    }

    /// Releases ordinary keys first, then modifiers, then mouse buttons, so no
    /// application ever sees a shortcut fire during cleanup.
    pub fn release_all(&mut self, out: &mut Vec<Output>) {
        let (plain, modifiers): (Vec<u16>, Vec<u16>) = self.keys.iter().partition(|k| !keymap::is_modifier(**k));
        for key in plain.into_iter().chain(modifiers) {
            self.keys.retain(|k| *k != key);
            out.push(Output::Key { key, down: false, repeat: false, mods: self.mods() });
        }
        for button in 0..32u8 {
            let bit = 1u32 << button;
            if self.buttons & bit != 0 {
                self.buttons &= !bit;
                out.push(Output::Button { button, down: false, clicks: 1, x: self.x, y: self.y, mods: self.mods() });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(inj: &mut Injector, msg: Message) -> Vec<Output> {
        let mut out = Vec::new();
        inj.apply(&msg, Instant::now(), &mut out);
        out.retain(|o| *o != Output::Wake);
        out
    }

    #[test]
    fn modifiers_are_reflected_on_following_events() {
        let mut inj = Injector::new();
        apply(&mut inj, Message::Key { key: hid::LEFT_META, down: true, repeat: false });
        let out = apply(&mut inj, Message::Key { key: 0x06, down: true, repeat: false });
        assert_eq!(out, vec![Output::Key { key: 0x06, down: true, repeat: false, mods: mods::META }]);
    }

    #[test]
    fn release_all_lets_go_of_everything_in_a_safe_order() {
        let mut inj = Injector::new();
        apply(&mut inj, Message::Key { key: hid::LEFT_META, down: true, repeat: false });
        apply(&mut inj, Message::Key { key: 0x04, down: true, repeat: false });
        apply(&mut inj, Message::MouseButton { button: 0, down: true, clicks: 1 });
        let out = apply(&mut inj, Message::ReleaseAll);
        assert_eq!(
            out,
            vec![
                Output::Key { key: 0x04, down: false, repeat: false, mods: mods::META },
                Output::Key { key: hid::LEFT_META, down: false, repeat: false, mods: 0 },
                Output::Button { button: 0, down: false, clicks: 1, x: 0.0, y: 0.0, mods: 0 },
            ]
        );
        assert!(!inj.holding_anything());
    }

    #[test]
    fn stray_releases_are_ignored_and_drags_report_buttons() {
        let mut inj = Injector::new();
        assert!(apply(&mut inj, Message::Key { key: 0x04, down: false, repeat: false }).is_empty());
        assert!(apply(&mut inj, Message::MouseButton { button: 1, down: false, clicks: 1 }).is_empty());
        apply(&mut inj, Message::MouseButton { button: 0, down: true, clicks: 1 });
        let out = apply(&mut inj, Message::MouseMove { x: 5.0, y: 6.0, dx: 1.0, dy: 1.0 });
        assert_eq!(out, vec![Output::MouseMove { x: 5.0, y: 6.0, dx: 1.0, dy: 1.0, buttons: 1, mods: 0 }]);
    }

    #[test]
    fn caps_lock_toggles() {
        let mut inj = Injector::new();
        apply(&mut inj, Message::Key { key: hid::CAPS_LOCK, down: true, repeat: false });
        apply(&mut inj, Message::Key { key: hid::CAPS_LOCK, down: false, repeat: false });
        assert_eq!(inj.mods(), mods::CAPS_LOCK);
    }

    #[test]
    fn display_wake_is_rate_limited() {
        let mut inj = Injector::new();
        let now = Instant::now();
        let mut out = Vec::new();
        inj.apply(&Message::MouseMove { x: 0.0, y: 0.0, dx: 0.0, dy: 0.0 }, now, &mut out);
        inj.apply(&Message::MouseMove { x: 0.0, y: 0.0, dx: 0.0, dy: 0.0 }, now, &mut out);
        assert_eq!(out.iter().filter(|o| **o == Output::Wake).count(), 1);
    }
}
