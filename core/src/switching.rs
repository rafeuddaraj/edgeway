//! Screen arrangement and the decision of which device receives input.
//!
//! All devices live in one *arrangement space* measured in points, with the
//! controlling device's own global coordinates as the reference, just like
//! System Settings › Displays. Each remote device is a group of screens
//! shifted into that space by its placement.
//!
//! The controlling device runs one [`Switcher`]. It sees every local input
//! event first and answers [`Verdict::Pass`] (deliver locally) or
//! [`Verdict::Swallow`] (the local system never sees it), plus a list of
//! [`Effect`]s to carry out. While another device has focus, the switcher owns a
//! virtual cursor in arrangement space and sends absolute positions, so the two
//! sides can never disagree about where the pointer is.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::identity::PeerId;
use crate::keymap::{self, hid, mods};
use crate::keystate::{KeyRouter, Route};
use crate::protocol::Message;

/// How far past an edge we look for a neighbouring screen.
const EDGE_PROBE: f64 = 2.0;
/// Pushing against an edge must be continuous; a pause this long starts over.
const PUSH_WINDOW: Duration = Duration::from_millis(400);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    fn offset(self, dx: f64, dy: f64) -> Point {
        Point { x: self.x + dx, y: self.y + dy }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Rect { x, y, w, h }
    }

    pub fn max_x(&self) -> f64 {
        self.x + self.w
    }

    pub fn max_y(&self) -> f64 {
        self.y + self.h
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.max_x() && p.y >= self.y && p.y < self.max_y()
    }

    /// Nearest addressable point inside the rectangle.
    pub fn clamp(&self, p: Point) -> Point {
        Point { x: p.x.clamp(self.x, (self.max_x() - 1.0).max(self.x)), y: p.y.clamp(self.y, (self.max_y() - 1.0).max(self.y)) }
    }

    pub fn translate(&self, dx: f64, dy: f64) -> Rect {
        Rect { x: self.x + dx, y: self.y + dy, ..*self }
    }

    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.max_x() && o.x < self.max_x() && self.y < o.max_y() && o.y < self.max_y()
    }

    fn distance_sq(&self, p: Point) -> f64 {
        let c = self.clamp(p);
        (c.x - p.x).powi(2) + (c.y - p.y).powi(2)
    }
}

/// Bounding box of a set of rectangles.
pub fn bounds(rects: &[Rect]) -> Option<Rect> {
    let first = rects.first()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.max_x(), first.max_y());
    for r in &rects[1..] {
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.max_x());
        y1 = y1.max(r.max_y());
    }
    Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
}

/// One device's screens in arrangement space.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    /// `None` for this device.
    pub peer: Option<PeerId>,
    pub screens: Vec<Rect>,
    /// arrangement = device coordinates + shift
    pub shift: Point,
}

impl Device {
    pub fn local(screens: Vec<Rect>) -> Self {
        Device { peer: None, screens, shift: Point::default() }
    }

    /// A remote device whose bounding box's top-left sits at `placement`.
    pub fn remote(peer: PeerId, screens_in_device_coords: &[Rect], placement: Point) -> Option<Self> {
        let b = bounds(screens_in_device_coords)?;
        let shift = Point::new(placement.x - b.x, placement.y - b.y);
        Some(Device {
            peer: Some(peer),
            screens: screens_in_device_coords.iter().map(|r| r.translate(shift.x, shift.y)).collect(),
            shift,
        })
    }

    pub fn contains(&self, p: Point) -> bool {
        self.screens.iter().any(|s| s.contains(p))
    }

    /// The screen containing `p`, or the nearest one.
    pub fn screen_for(&self, p: Point) -> Rect {
        self.screens
            .iter()
            .copied()
            .find(|s| s.contains(p))
            .or_else(|| {
                self.screens.iter().copied().min_by(|a, b| a.distance_sq(p).total_cmp(&b.distance_sq(p)))
            })
            .unwrap_or(Rect::new(0.0, 0.0, 1.0, 1.0))
    }

    pub fn to_device(&self, p: Point) -> Point {
        Point::new(p.x - self.shift.x, p.y - self.shift.y)
    }

    pub fn from_device(&self, p: Point) -> Point {
        Point::new(p.x + self.shift.x, p.y + self.shift.y)
    }
}

/// Every device that can currently receive input. Index 0 is always local.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub devices: Vec<Device>,
}

impl Layout {
    pub fn new(local_screens: Vec<Rect>) -> Self {
        Layout { devices: vec![Device::local(local_screens)] }
    }

    pub fn index_of(&self, peer: &PeerId) -> Option<usize> {
        self.devices.iter().position(|d| d.peer.as_ref() == Some(peer))
    }

    fn device_at(&self, p: Point, exclude: usize) -> Option<usize> {
        self.devices.iter().enumerate().find(|(i, d)| *i != exclude && d.contains(p)).map(|(i, _)| i)
    }
}

/// Where to put a newly paired device: above this one, centred, by default —
/// then to the right, left or below if that space is taken.
pub fn default_placement(occupied: &[Rect], local: Rect, size: (f64, f64)) -> Point {
    let (w, h) = size;
    let candidates = [
        Point::new(local.x + (local.w - w) / 2.0, local.y - h),
        Point::new(local.max_x(), local.y + (local.h - h) / 2.0),
        Point::new(local.x - w, local.y + (local.h - h) / 2.0),
        Point::new(local.x + (local.w - w) / 2.0, local.max_y()),
    ];
    for c in candidates {
        let c = Point::new(c.x.round(), c.y.round());
        let r = Rect::new(c.x, c.y, w, h);
        if !occupied.iter().any(|o| o.intersects(&r)) {
            return c;
        }
    }
    let top = occupied.iter().map(|r| r.y).fold(local.y, f64::min);
    Point::new((local.x + (local.w - w) / 2.0).round(), (top - h).round())
}

/// A local input event, already translated to neutral codes by the shell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LocalEvent {
    /// `x`/`y`: where the system cursor is now; `dx`/`dy`: raw movement.
    MouseMove { x: f64, y: f64, dx: f64, dy: f64 },
    Button { button: u8, down: bool, clicks: u8 },
    Scroll { dx: f64, dy: f64, lines_x: i16, lines_y: i16, phase: u8, momentum: u8, continuous: bool },
    Key { key: u16, down: bool, repeat: bool },
    /// Gestures and anything else: never delivered locally while away.
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Swallow,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Send(PeerId, Message),
    /// Freeze and hide the local cursor here.
    Freeze { at: Point },
    /// Show the local cursor again at this point.
    Unfreeze { warp_to: Point },
    /// Input focus changed (`None` = this device).
    Focus(Option<PeerId>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HomeReason {
    Hotkey,
    Paused,
    SecureInput,
    Requested,
    Layout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dir {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Default)]
struct Push {
    dir: Option<Dir>,
    amount: f64,
    at: Option<Instant>,
}

struct Hit {
    dir: Dir,
    overshoot: f64,
    dest: usize,
    entry: Point,
}

pub struct Switcher {
    layout: Layout,
    focus: usize,
    /// Arrangement coordinates. While local: the last seen system cursor.
    cursor: Point,
    /// Where the local cursor waits, frozen and hidden, while away.
    parked_at: Point,
    resistance: f64,
    push: Push,
    buttons: u32,
    keys: KeyRouter,
    held_modifiers: HashSet<u16>,
    paused: bool,
}

impl Switcher {
    pub fn new(local_screens: Vec<Rect>, resistance: f64) -> Self {
        Switcher {
            layout: Layout::new(local_screens),
            focus: 0,
            cursor: Point::default(),
            parked_at: Point::default(),
            resistance: resistance.max(0.0),
            push: Push::default(),
            buttons: 0,
            keys: KeyRouter::new(),
            held_modifiers: HashSet::new(),
            paused: false,
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn focus(&self) -> Option<PeerId> {
        self.layout.devices.get(self.focus).and_then(|d| d.peer)
    }

    pub fn set_resistance(&mut self, r: f64) {
        self.resistance = r.max(0.0);
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn set_paused(&mut self, paused: bool, out: &mut Vec<Effect>) {
        self.paused = paused;
        if paused {
            self.go_home(HomeReason::Paused, out);
        }
    }

    /// Replaces the arrangement, keeping focus on the same device if it is
    /// still present.
    pub fn set_layout(&mut self, layout: Layout, out: &mut Vec<Effect>) {
        if let Some(peer) = self.focus() {
            if let Some(idx) = layout.index_of(&peer) {
                let in_device = self.layout.devices[self.focus].to_device(self.cursor);
                let dev = &layout.devices[idx];
                let p = dev.from_device(in_device);
                self.cursor = dev.screen_for(p).clamp(p);
                self.parked_at = layout.devices[0].screen_for(self.parked_at).clamp(self.parked_at);
                self.layout = layout;
                self.focus = idx;
                return;
            }
            self.go_home(HomeReason::Layout, out);
        }
        self.layout = layout;
        self.focus = 0;
    }

    /// A connection ended. The peer releases its own keys; we just come home.
    pub fn peer_lost(&mut self, peer: &PeerId, out: &mut Vec<Effect>) {
        self.keys.drop_peer(peer);
        if self.focus() == Some(*peer) {
            self.focus = 0;
            self.cursor = self.parked_at;
            self.push = Push::default();
            out.push(Effect::Unfreeze { warp_to: self.parked_at });
            out.push(Effect::Focus(None));
        }
    }

    /// Brings input back to this device immediately, releasing everything the
    /// remote device holds (used for the hotkey, pause and secure input).
    pub fn go_home(&mut self, _reason: HomeReason, out: &mut Vec<Effect>) {
        let Some(peer) = self.focus() else { return };
        out.push(Effect::Send(peer, Message::Leave));
        out.push(Effect::Send(peer, Message::ReleaseAll));
        self.keys.drop_peer(&peer);
        self.focus = 0;
        self.cursor = self.parked_at;
        self.push = Push::default();
        out.push(Effect::Unfreeze { warp_to: self.parked_at });
        out.push(Effect::Focus(None));
    }

    pub fn handle(&mut self, ev: &LocalEvent, now: Instant, out: &mut Vec<Effect>) -> Verdict {
        match *ev {
            LocalEvent::MouseMove { x, y, dx, dy } => {
                if self.focus == 0 {
                    self.local_move(Point::new(x, y), dx, dy, now, out)
                } else {
                    self.remote_move(dx, dy, now, out)
                }
            }
            LocalEvent::Button { button, down, clicks } => {
                let bit = 1u32 << button.min(31);
                if down {
                    self.buttons |= bit;
                } else {
                    self.buttons &= !bit;
                }
                match self.focus() {
                    None => Verdict::Pass,
                    Some(peer) => {
                        out.push(Effect::Send(peer, Message::MouseButton { button, down, clicks }));
                        Verdict::Swallow
                    }
                }
            }
            LocalEvent::Scroll { dx, dy, lines_x, lines_y, phase, momentum, continuous } => match self.focus() {
                None => Verdict::Pass,
                Some(peer) => {
                    out.push(Effect::Send(
                        peer,
                        Message::Scroll { dx: dx as f32, dy: dy as f32, lines_x, lines_y, phase, momentum, continuous },
                    ));
                    Verdict::Swallow
                }
            },
            LocalEvent::Key { key, down, repeat } => self.key(key, down, repeat, out),
            LocalEvent::Other => {
                if self.focus == 0 {
                    Verdict::Pass
                } else {
                    Verdict::Swallow
                }
            }
        }
    }

    fn held_mods(&self) -> u32 {
        self.held_modifiers.iter().fold(0, |m, k| m | keymap::modifier_bit(*k))
    }

    fn key(&mut self, key: u16, down: bool, repeat: bool, out: &mut Vec<Effect>) -> Verdict {
        if keymap::is_modifier(key) {
            if down {
                self.held_modifiers.insert(key);
            } else {
                self.held_modifiers.remove(&key);
            }
        }
        // ⌃⌥⌘H always brings control home.
        let home_combo = mods::CONTROL | mods::ALT | mods::META;
        if down && !repeat && key == hid::H && self.focus != 0 && self.held_mods() & home_combo == home_combo {
            self.go_home(HomeReason::Hotkey, out);
            return Verdict::Swallow;
        }
        match self.keys.route(key, down, repeat, self.focus()) {
            Route::Local => Verdict::Pass,
            Route::Remote(peer) => {
                out.push(Effect::Send(peer, Message::Key { key, down, repeat }));
                Verdict::Swallow
            }
        }
    }

    fn can_cross(&self) -> bool {
        !self.paused && self.buttons == 0 && self.layout.devices.len() > 1
    }

    fn local_move(&mut self, p: Point, dx: f64, dy: f64, now: Instant, out: &mut Vec<Effect>) -> Verdict {
        self.cursor = p;
        if !self.can_cross() || self.layout.devices[0].contains(p.offset(dx, dy)) {
            self.push = Push::default();
            return Verdict::Pass;
        }
        match self.probe(0, p, dx, dy) {
            Some(hit) => {
                if self.accumulate(hit.dir, hit.overshoot, now) {
                    self.cross(hit, out);
                    Verdict::Swallow
                } else {
                    Verdict::Pass
                }
            }
            None => {
                self.push = Push::default();
                Verdict::Pass
            }
        }
    }

    fn remote_move(&mut self, dx: f64, dy: f64, now: Instant, out: &mut Vec<Effect>) -> Verdict {
        let p = self.cursor;
        let t = p.offset(dx, dy);
        let dev = &self.layout.devices[self.focus];
        let next = if dev.contains(t) {
            self.push = Push::default();
            t
        } else {
            let clamped = dev.screen_for(p).clamp(t);
            let hit = if self.can_cross() { self.probe(self.focus, p, dx, dy) } else { None };
            match hit {
                Some(hit) => {
                    if self.accumulate(hit.dir, hit.overshoot, now) {
                        self.cross(hit, out);
                        return Verdict::Swallow;
                    }
                }
                None => self.push = Push::default(),
            }
            clamped
        };
        self.cursor = next;
        let dev = &self.layout.devices[self.focus];
        let d = dev.to_device(next);
        if let Some(peer) = dev.peer {
            out.push(Effect::Send(
                peer,
                Message::MouseMove { x: d.x as f32, y: d.y as f32, dx: dx as f32, dy: dy as f32 },
            ));
        }
        Verdict::Swallow
    }

    /// Looks just past the edge being pushed for another device's screen.
    fn probe(&self, from: usize, p: Point, dx: f64, dy: f64) -> Option<Hit> {
        let dev = &self.layout.devices[from];
        let s = dev.screen_for(p);
        let inside = s.clamp(p);
        let mut candidates: Vec<(Dir, Point, f64)> = Vec::with_capacity(2);
        if dx < 0.0 && p.x + dx < s.x {
            candidates.push((Dir::Left, Point::new(s.x - EDGE_PROBE, inside.y), s.x - (p.x + dx)));
        }
        if dx > 0.0 && p.x + dx > s.max_x() - 1.0 {
            candidates.push((Dir::Right, Point::new(s.max_x() - 1.0 + EDGE_PROBE, inside.y), p.x + dx - (s.max_x() - 1.0)));
        }
        if dy < 0.0 && p.y + dy < s.y {
            candidates.push((Dir::Up, Point::new(inside.x, s.y - EDGE_PROBE), s.y - (p.y + dy)));
        }
        if dy > 0.0 && p.y + dy > s.max_y() - 1.0 {
            candidates.push((Dir::Down, Point::new(inside.x, s.max_y() - 1.0 + EDGE_PROBE), p.y + dy - (s.max_y() - 1.0)));
        }
        candidates.into_iter().find_map(|(dir, q, overshoot)| {
            if dev.contains(q) {
                return None; // another screen of the same device
            }
            let dest = self.layout.device_at(q, from)?;
            let entry = self.layout.devices[dest].screen_for(q).clamp(q);
            Some(Hit { dir, overshoot: overshoot.max(0.0), dest, entry })
        })
    }

    fn accumulate(&mut self, dir: Dir, amount: f64, now: Instant) -> bool {
        let stale = self.push.at.map_or(true, |t| now.saturating_duration_since(t) > PUSH_WINDOW);
        if self.push.dir != Some(dir) || stale {
            self.push.amount = 0.0;
        }
        self.push.dir = Some(dir);
        self.push.amount += amount;
        self.push.at = Some(now);
        self.push.amount >= self.resistance
    }

    fn cross(&mut self, hit: Hit, out: &mut Vec<Effect>) {
        let from = self.focus;
        if from == 0 {
            self.parked_at = self.layout.devices[0].screen_for(self.cursor).clamp(self.cursor);
            out.push(Effect::Freeze { at: self.parked_at });
        } else if let Some(prev) = self.layout.devices[from].peer {
            out.push(Effect::Send(prev, Message::Leave));
        }
        self.focus = hit.dest;
        self.cursor = hit.entry;
        self.push = Push::default();
        let dev = &self.layout.devices[hit.dest];
        match dev.peer {
            None => {
                out.push(Effect::Unfreeze { warp_to: hit.entry });
                out.push(Effect::Focus(None));
            }
            Some(peer) => {
                let d = dev.to_device(hit.entry);
                out.push(Effect::Send(peer, Message::Enter { x: d.x as f32, y: d.y as f32 }));
                out.push(Effect::Focus(Some(peer)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer() -> PeerId {
        PeerId([7; 32])
    }

    /// MacBook 1512×982 at the origin; Mac mini 2560×1440 centred above it.
    fn setup(resistance: f64) -> Switcher {
        let local = Rect::new(0.0, 0.0, 1512.0, 982.0);
        let mut sw = Switcher::new(vec![local], resistance);
        let mini = [Rect::new(0.0, 0.0, 2560.0, 1440.0)];
        let place = default_placement(&[local], local, (2560.0, 1440.0));
        assert_eq!(place, Point::new(-524.0, -1440.0));
        let mut layout = Layout::new(vec![local]);
        layout.devices.push(Device::remote(peer(), &mini, place).unwrap());
        sw.set_layout(layout, &mut Vec::new());
        sw
    }

    fn mv(sw: &mut Switcher, x: f64, y: f64, dx: f64, dy: f64, out: &mut Vec<Effect>) -> Verdict {
        sw.handle(&LocalEvent::MouseMove { x, y, dx, dy }, Instant::now(), out)
    }

    #[test]
    fn crosses_up_only_after_pushing_through_the_resistance() {
        let mut sw = setup(40.0);
        let mut out = Vec::new();
        assert_eq!(mv(&mut sw, 700.0, 0.0, 0.0, -15.0, &mut out), Verdict::Pass);
        assert_eq!(mv(&mut sw, 700.0, 0.0, 0.0, -15.0, &mut out), Verdict::Pass);
        assert!(out.is_empty());
        assert_eq!(mv(&mut sw, 700.0, 0.0, 0.0, -15.0, &mut out), Verdict::Swallow);
        assert_eq!(sw.focus(), Some(peer()));
        assert_eq!(out[0], Effect::Freeze { at: Point::new(700.0, 0.0) });
        // 700 in MacBook space is 700 + 524 on the mini, just above its bottom edge.
        assert_eq!(out[1], Effect::Send(peer(), Message::Enter { x: 1224.0, y: 1438.0 }));
    }

    #[test]
    fn moving_away_from_the_edge_resets_the_push() {
        let mut sw = setup(40.0);
        let mut out = Vec::new();
        mv(&mut sw, 700.0, 0.0, 0.0, -30.0, &mut out);
        mv(&mut sw, 700.0, 0.0, 0.0, 5.0, &mut out);
        assert_eq!(mv(&mut sw, 700.0, 0.0, 0.0, -30.0, &mut out), Verdict::Pass);
        assert_eq!(sw.focus(), None);
    }

    #[test]
    fn remote_pointer_is_clamped_and_returns_to_the_matching_spot() {
        let mut sw = setup(0.0);
        let mut out = Vec::new();
        mv(&mut sw, 100.0, 0.0, 0.0, -5.0, &mut out);
        assert_eq!(sw.focus(), Some(peer()));
        out.clear();

        // Far left on the mini is outside the MacBook's span: just clamps.
        assert_eq!(mv(&mut sw, 0.0, 0.0, -5000.0, 0.0, &mut out), Verdict::Swallow);
        assert_eq!(out, vec![Effect::Send(peer(), Message::MouseMove { x: 0.0, y: 1438.0, dx: -5000.0, dy: 0.0 })]);
        out.clear();
        mv(&mut sw, 0.0, 0.0, 0.0, 50.0, &mut out); // bottom edge, no MacBook below x=-524
        assert_eq!(sw.focus(), Some(peer()));
        out.clear();

        // Move right into the overlap and come back down.
        mv(&mut sw, 0.0, 0.0, 1000.0, 0.0, &mut out);
        out.clear();
        mv(&mut sw, 0.0, 0.0, 0.0, 10.0, &mut out);
        assert_eq!(sw.focus(), None);
        assert_eq!(out[0], Effect::Send(peer(), Message::Leave));
        assert_eq!(out[1], Effect::Unfreeze { warp_to: Point::new(476.0, 1.0) });
    }

    #[test]
    fn everything_is_swallowed_and_forwarded_while_away() {
        let mut sw = setup(0.0);
        let mut out = Vec::new();
        mv(&mut sw, 100.0, 0.0, 0.0, -5.0, &mut out);
        out.clear();
        let now = Instant::now();
        let key = LocalEvent::Key { key: 0x04, down: true, repeat: false };
        assert_eq!(sw.handle(&key, now, &mut out), Verdict::Swallow);
        let click = LocalEvent::Button { button: 0, down: true, clicks: 1 };
        assert_eq!(sw.handle(&click, now, &mut out), Verdict::Swallow);
        assert_eq!(sw.handle(&LocalEvent::Other, now, &mut out), Verdict::Swallow);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn never_crosses_while_a_button_is_held() {
        let mut sw = setup(0.0);
        let mut out = Vec::new();
        let now = Instant::now();
        sw.handle(&LocalEvent::Button { button: 0, down: true, clicks: 1 }, now, &mut out);
        assert_eq!(mv(&mut sw, 100.0, 0.0, 0.0, -50.0, &mut out), Verdict::Pass);
        assert_eq!(sw.focus(), None);
    }

    #[test]
    fn keys_pressed_before_crossing_are_released_locally() {
        let mut sw = setup(0.0);
        let mut out = Vec::new();
        let now = Instant::now();
        let shift_down = LocalEvent::Key { key: hid::LEFT_SHIFT, down: true, repeat: false };
        assert_eq!(sw.handle(&shift_down, now, &mut out), Verdict::Pass);
        mv(&mut sw, 100.0, 0.0, 0.0, -5.0, &mut out);
        let shift_up = LocalEvent::Key { key: hid::LEFT_SHIFT, down: false, repeat: false };
        assert_eq!(sw.handle(&shift_up, now, &mut out), Verdict::Pass);
    }

    #[test]
    fn hotkey_brings_control_home_and_releases_remote_keys() {
        let mut sw = setup(0.0);
        let mut out = Vec::new();
        let now = Instant::now();
        mv(&mut sw, 100.0, 0.0, 0.0, -5.0, &mut out);
        for k in [hid::LEFT_CONTROL, hid::LEFT_ALT, hid::LEFT_META] {
            sw.handle(&LocalEvent::Key { key: k, down: true, repeat: false }, now, &mut out);
        }
        out.clear();
        let h = LocalEvent::Key { key: hid::H, down: true, repeat: false };
        assert_eq!(sw.handle(&h, now, &mut out), Verdict::Swallow);
        assert_eq!(sw.focus(), None);
        assert!(out.contains(&Effect::Send(peer(), Message::ReleaseAll)));
        assert!(out.contains(&Effect::Unfreeze { warp_to: Point::new(100.0, 0.0) }));
        // Modifier releases now stay local; the mini already released them.
        let up = LocalEvent::Key { key: hid::LEFT_META, down: false, repeat: false };
        assert_eq!(sw.handle(&up, now, &mut out), Verdict::Pass);
    }

    #[test]
    fn losing_the_peer_unfreezes_the_local_cursor() {
        let mut sw = setup(0.0);
        let mut out = Vec::new();
        mv(&mut sw, 300.0, 0.0, 0.0, -5.0, &mut out);
        out.clear();
        sw.peer_lost(&peer(), &mut out);
        assert_eq!(sw.focus(), None);
        assert_eq!(out[0], Effect::Unfreeze { warp_to: Point::new(300.0, 0.0) });
    }

    #[test]
    fn pause_returns_home_and_blocks_crossing() {
        let mut sw = setup(0.0);
        let mut out = Vec::new();
        mv(&mut sw, 300.0, 0.0, 0.0, -5.0, &mut out);
        sw.set_paused(true, &mut out);
        assert_eq!(sw.focus(), None);
        assert_eq!(mv(&mut sw, 300.0, 0.0, 0.0, -50.0, &mut out), Verdict::Pass);
        assert_eq!(sw.focus(), None);
    }

    #[test]
    fn default_placement_avoids_occupied_space() {
        let local = Rect::new(0.0, 0.0, 100.0, 100.0);
        let above = Rect::new(0.0, -100.0, 100.0, 100.0);
        assert_eq!(default_placement(&[local, above], local, (100.0, 100.0)), Point::new(100.0, 0.0));
    }
}
