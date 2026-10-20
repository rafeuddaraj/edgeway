//! Connection lifecycle and the public API used by the platform shells.
//!
//! Roles:
//!
//! * **Primary** (the device whose keyboard and trackpad you use) browses for
//!   devices, dials the trusted ones, keeps them connected and routes local
//!   input through the [`Switcher`].
//! * **Secondary** (the device being controlled) advertises itself, accepts
//!   connections, shows the pairing code and injects what it receives.
//!
//! Threads: one listener (secondary), one dialer (primary), one heartbeat, and
//! a reader plus a writer per connection. All of them block on sockets or
//! condition variables, so an idle Edgeway uses no CPU. The only periodic work
//! is a one-second heartbeat while at least one device is connected.

use std::collections::HashMap;
use std::io::{self, BufWriter, Write};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::thread;
use std::time::{Duration, Instant};

use nanoserde::SerJson;

use crate::config::{Config, Placement, ScreenRecord};
use crate::discovery::{self, Advert, Discovery, Found};
use crate::identity::{Identity, PeerId};
use crate::injector::{Injector, Output};
use crate::noise::{self, Opener, Sealer};
use crate::pairing::{self, Side, Throttle};
use crate::protocol::{Hello, Message, PairReject, Platform, Screen, PROTOCOL_VERSION};
use crate::switching::{self, Device, Effect, HomeReason, Layout, LocalEvent, Point, Rect, Switcher, Verdict};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const PAIR_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const PAIR_CODE_TIMEOUT: Duration = Duration::from_secs(180);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
/// No traffic for this long ends a session and releases every held key.
const HEARTBEAT_TIMEOUT_MS: u64 = 3500;
const MAX_PENDING_INBOUND: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Shares this device's keyboard and pointer.
    Primary,
    /// Is controlled by another device.
    Secondary,
}

impl Role {
    fn label(self) -> &'static str {
        match self {
            Role::Primary => "primary",
            Role::Secondary => "secondary",
        }
    }
}

pub struct EngineOptions {
    /// Where `identity.key` and `config.json` live.
    pub dir: PathBuf,
    pub role: Role,
    /// Name shown to other devices, e.g. "Rafe’s MacBook Pro".
    pub name: String,
    pub screens: Vec<Screen>,
    /// Listen port override (secondary). `Some(0)` picks a free port.
    pub port: Option<u16>,
    /// Bind to 127.0.0.1 only (tests).
    pub loopback_only: bool,
    /// Advertise / browse with DNS-SD.
    pub discovery: bool,
    /// Fixed pairing code for automated tests. Never set in production.
    pub fixed_pairing_code: Option<String>,
}

impl EngineOptions {
    pub fn new(dir: PathBuf, role: Role, name: String) -> Self {
        EngineOptions {
            dir,
            role,
            name,
            screens: Vec::new(),
            port: None,
            loopback_only: false,
            discovery: true,
            fixed_pairing_code: None,
        }
    }
}

/// Primary-side cursor control requested by the switcher.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Control {
    /// Freeze and hide the local cursor at this point.
    Freeze { x: f64, y: f64 },
    /// Show the local cursor again here.
    Unfreeze { x: f64, y: f64 },
}

/// What the pairing UI should show.
#[derive(Clone, Debug, PartialEq)]
pub enum PairingUi {
    /// Secondary: display this code large and clear.
    ShowCode { peer: String, code: String },
    /// Primary: ask the person for the code shown on `peer`.
    EnterCode { peer: String },
    Succeeded { peer: String },
    Failed { peer: String, reason: String },
    /// Dismiss any pairing UI.
    Closed,
}

/// Implemented by the platform shell. Called from engine threads.
pub trait Host: Send + Sync + 'static {
    /// Secondary: post this event into the local session.
    fn inject(&self, ev: &Output);
    /// Primary: freeze or release the local cursor.
    fn control(&self, c: Control);
    /// Something visible changed; fetch a new [`Snapshot`].
    fn state_changed(&self);
    fn pairing(&self, ev: PairingUi);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Status {
    #[default]
    Offline,
    Connecting,
    Connected,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Offline => "offline",
            Status::Connecting => "connecting",
            Status::Connected => "connected",
        }
    }
}

struct SessionHandle {
    id: u64,
    tx: Sender<Message>,
    stream: TcpStream,
    last_rx: Arc<AtomicU64>,
}

#[derive(Default)]
struct PeerState {
    status: Status,
    session: Option<SessionHandle>,
    name: String,
    platform: Option<Platform>,
    screens: Vec<Screen>,
    rtt_ms: Option<f32>,
    attempts: u32,
    retry_at: Option<Instant>,
    error: Option<String>,
}

enum PairingSlot {
    Idle,
    /// Secondary is displaying a code.
    Showing { peer: String, code: String, stream: TcpStream },
    /// Primary is waiting for the person to type the code.
    AwaitingCode { peer: String, tx: Sender<String>, stream: TcpStream },
    /// Pairing messages are in flight.
    Working { peer: String, stream: Option<TcpStream> },
}

/// One authenticated connection before it becomes a session.
struct Conn {
    stream: TcpStream,
    sealer: Sealer,
    opener: Opener,
    remote: PeerId,
    hash: [u8; 32],
}

impl Conn {
    fn send(&mut self, m: &Message) -> io::Result<()> {
        self.sealer.send(&mut self.stream, m)?;
        self.stream.flush()
    }

    fn recv(&mut self, timeout: Duration) -> io::Result<Message> {
        self.stream.set_read_timeout(Some(timeout))?;
        self.opener.recv(&mut self.stream)
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn protocol_error(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

/// Low-latency socket options. `SO_NOSIGPIPE` matters: inside an app, Rust's
/// usual `SIGPIPE` handling is not installed, and a write to a closed socket
/// would otherwise terminate the process.
fn tune(stream: &TcpStream) {
    let _ = stream.set_nodelay(true);
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let fd = stream.as_raw_fd();
        let one: libc::c_int = 1;
        // DSCP EF: Wi-Fi puts these packets in the voice access category.
        let tos: libc::c_int = 0xB8;
        unsafe {
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_NOSIGPIPE,
                &one as *const _ as *const libc::c_void,
                std::mem::size_of_val(&one) as libc::socklen_t,
            );
            let size = std::mem::size_of_val(&tos) as libc::socklen_t;
            libc::setsockopt(fd, libc::IPPROTO_IP, libc::IP_TOS, &tos as *const _ as *const libc::c_void, size);
            libc::setsockopt(fd, libc::IPPROTO_IPV6, libc::IPV6_TCLASS, &tos as *const _ as *const libc::c_void, size);
        }
        let _ = one;
    }
}

fn screens_to_rects(screens: &[Screen]) -> Vec<Rect> {
    screens.iter().map(|s| Rect::new(s.x as f64, s.y as f64, s.w as f64, s.h as f64)).collect()
}

fn backoff(attempts: u32) -> Duration {
    Duration::from_millis((500u64 << attempts.min(5)).min(15_000))
}

struct Inner {
    role: Role,
    name: String,
    dir: PathBuf,
    loopback_only: bool,
    fixed_code: Option<String>,
    identity: Identity,
    host: Arc<dyn Host>,
    epoch: Instant,

    config: Mutex<Config>,
    screens: Mutex<Vec<Screen>>,
    peers: Mutex<HashMap<PeerId, PeerState>>,
    nearby: Mutex<Vec<Found>>,
    switcher: Mutex<Switcher>,
    injector: Mutex<Injector>,
    controlled_by: Mutex<Option<PeerId>>,
    pairing: Mutex<PairingSlot>,
    throttle: Mutex<Throttle>,
    pending_inbound: AtomicU64,

    paused: AtomicBool,
    shutdown: AtomicBool,
    port: AtomicU16,
    next_session: AtomicU64,
    wake: (Mutex<u64>, Condvar),
}

/// The running engine. Dropping it stops everything.
pub struct Engine {
    inner: Arc<Inner>,
    discovery: Mutex<Option<Discovery>>,
}

impl Engine {
    pub fn start(opts: EngineOptions, host: Arc<dyn Host>) -> io::Result<Engine> {
        std::fs::create_dir_all(&opts.dir)?;
        let identity = Identity::load_or_create(&opts.dir)?;
        let config = Config::load(&opts.dir);
        let resistance = config.edge_resistance as f64;
        let inner = Arc::new(Inner {
            role: opts.role,
            name: opts.name.clone(),
            dir: opts.dir.clone(),
            loopback_only: opts.loopback_only,
            fixed_code: opts.fixed_pairing_code.clone(),
            identity,
            host,
            epoch: Instant::now(),
            switcher: Mutex::new(Switcher::new(screens_to_rects(&opts.screens), resistance)),
            config: Mutex::new(config),
            screens: Mutex::new(opts.screens.clone()),
            peers: Mutex::new(HashMap::new()),
            nearby: Mutex::new(Vec::new()),
            injector: Mutex::new(Injector::new()),
            controlled_by: Mutex::new(None),
            pairing: Mutex::new(PairingSlot::Idle),
            throttle: Mutex::new(Throttle::new()),
            pending_inbound: AtomicU64::new(0),
            paused: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            port: AtomicU16::new(0),
            next_session: AtomicU64::new(1),
            wake: (Mutex::new(0), Condvar::new()),
        });

        let mut advert = None;
        match opts.role {
            Role::Secondary => {
                let port = inner.start_listener(opts.port)?;
                advert = Some(Advert { name: opts.name.clone(), port, fingerprint: inner.identity.id().fingerprint() });
            }
            Role::Primary => {
                inner.rebuild_layout();
                let i = inner.clone();
                thread::Builder::new().name("edgeway-dialer".into()).spawn(move || i.dialer())?;
            }
        }
        let i = inner.clone();
        thread::Builder::new().name("edgeway-heartbeat".into()).spawn(move || i.heartbeat())?;

        let discovery = if opts.discovery {
            let weak: Weak<Inner> = Arc::downgrade(&inner);
            let sink = Box::new(move |ev: discovery::Event| {
                if let Some(inner) = weak.upgrade() {
                    inner.on_discovery(ev);
                }
            });
            Some(Discovery::start(advert, opts.role == Role::Primary, sink)?)
        } else {
            None
        };
        ew_log!("started as {} ({})", opts.role.label(), inner.identity.id().short());
        Ok(Engine { inner, discovery: Mutex::new(discovery) })
    }

    /// Stops discovery, says goodbye to every peer and closes all connections.
    pub fn stop(&self) {
        if self.inner.shutdown.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(mut d) = lock(&self.discovery).take() {
            d.stop();
        }
        let sessions: Vec<(Sender<Message>, TcpStream)> = lock(&self.inner.peers)
            .values()
            .filter_map(|p| p.session.as_ref())
            .filter_map(|s| s.stream.try_clone().ok().map(|st| (s.tx.clone(), st)))
            .collect();
        for (tx, _) in &sessions {
            let _ = tx.send(Message::Bye);
        }
        thread::sleep(Duration::from_millis(20));
        for (_, stream) in sessions {
            let _ = stream.shutdown(Shutdown::Both);
        }
        self.inner.cancel_pairing();
        self.inner.notify();
        let port = self.inner.port.load(Ordering::SeqCst);
        if self.inner.role == Role::Secondary && port != 0 {
            // Unblock `accept`.
            let _ = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(200));
        }
    }

    pub fn id(&self) -> PeerId {
        self.inner.identity.id()
    }

    /// Port the secondary is listening on.
    pub fn port(&self) -> u16 {
        self.inner.port.load(Ordering::SeqCst)
    }

    /// Primary hot path, called for every local input event.
    pub fn handle_local_event(&self, ev: &LocalEvent) -> Verdict {
        if self.inner.role != Role::Primary {
            return Verdict::Pass;
        }
        let mut effects = Vec::new();
        let verdict = lock(&self.inner.switcher).handle(ev, Instant::now(), &mut effects);
        if !effects.is_empty() {
            self.inner.apply(effects);
        }
        verdict
    }

    /// The local displays changed.
    pub fn set_screens(&self, screens: Vec<Screen>) {
        *lock(&self.inner.screens) = screens.clone();
        self.inner.broadcast(Message::Screens(screens));
        if self.inner.role == Role::Primary {
            self.inner.rebuild_layout();
        }
        self.inner.host.state_changed();
    }

    /// Feeds a discovery result (also used by tests to skip mDNS).
    pub fn discovered(&self, found: Found) {
        self.inner.on_discovery(discovery::Event::Found(found));
    }

    /// Primary: starts pairing with a device found on the network.
    pub fn pair(&self, instance: &str) -> bool {
        self.inner.clone().start_pairing(instance)
    }

    /// Primary: the code the person typed.
    pub fn submit_pairing_code(&self, code: &str) -> bool {
        let Some(code) = pairing::normalize_code(code) else { return false };
        match &*lock(&self.inner.pairing) {
            PairingSlot::AwaitingCode { tx, .. } => tx.send(code).is_ok(),
            _ => false,
        }
    }

    pub fn cancel_pairing(&self) {
        self.inner.cancel_pairing();
    }

    /// Removes a trusted device and disconnects it.
    pub fn forget(&self, id: &PeerId) {
        let removed = {
            let mut cfg = lock(&self.inner.config);
            let removed = cfg.forget(id);
            if removed {
                let _ = cfg.save(&self.inner.dir);
            }
            removed
        };
        if let Some(state) = lock(&self.inner.peers).remove(id) {
            if let Some(s) = state.session {
                let _ = s.tx.send(Message::Bye);
                let _ = s.stream.shutdown(Shutdown::Both);
            }
        }
        if removed && self.inner.role == Role::Primary {
            self.inner.rebuild_layout();
        }
        self.inner.host.state_changed();
    }

    /// Moves a device in the arrangement (top-left of its displays, in this
    /// device's coordinates).
    pub fn set_placement(&self, id: &PeerId, x: f32, y: f32) {
        {
            let mut cfg = lock(&self.inner.config);
            let Some(peer) = cfg.peer_mut(id) else { return };
            peer.placement = Some(Placement { x: x.round(), y: y.round() });
            let _ = cfg.save(&self.inner.dir);
        }
        self.inner.rebuild_layout();
        self.inner.host.state_changed();
    }

    pub fn set_paused(&self, paused: bool) {
        self.inner.paused.store(paused, Ordering::SeqCst);
        match self.inner.role {
            Role::Primary => {
                let mut effects = Vec::new();
                lock(&self.inner.switcher).set_paused(paused, &mut effects);
                self.inner.apply(effects);
            }
            Role::Secondary => {
                if paused {
                    self.inner.release_injected();
                }
            }
        }
        self.inner.host.state_changed();
    }

    /// Primary: bring input back to this device now.
    pub fn go_home(&self, reason: HomeReason) {
        let mut effects = Vec::new();
        lock(&self.inner.switcher).go_home(reason, &mut effects);
        self.inner.apply(effects);
    }

    pub fn set_edge_resistance(&self, points: f32) {
        let points = if points.is_finite() { points.clamp(0.0, 500.0) } else { crate::config::DEFAULT_EDGE_RESISTANCE };
        {
            let mut cfg = lock(&self.inner.config);
            cfg.edge_resistance = points;
            let _ = cfg.save(&self.inner.dir);
        }
        lock(&self.inner.switcher).set_resistance(points as f64);
        self.inner.host.state_changed();
    }

    /// The network changed or the Mac woke up: retry immediately.
    pub fn network_changed(&self) {
        for p in lock(&self.inner.peers).values_mut() {
            p.attempts = 0;
            p.retry_at = None;
        }
        self.inner.notify();
    }

    /// Primary: which device currently receives input (`None` = this one).
    pub fn focus(&self) -> Option<PeerId> {
        lock(&self.inner.switcher).focus()
    }

    pub fn snapshot(&self) -> Snapshot {
        self.inner.snapshot()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Inner {
    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    fn now_us(&self) -> u64 {
        self.epoch.elapsed().as_micros() as u64
    }

    fn notify(&self) {
        let (m, cv) = &self.wake;
        *lock(m) += 1;
        cv.notify_all();
    }

    /// Sleeps until notified or `timeout` (forever if `None`).
    fn wait(&self, timeout: Option<Duration>) {
        let (m, cv) = &self.wake;
        let guard = lock(m);
        let generation = *guard;
        match timeout {
            Some(t) => {
                drop(cv.wait_timeout_while(guard, t, |g| *g == generation));
            }
            None => {
                drop(cv.wait_while(guard, |g| *g == generation));
            }
        }
    }

    // ----- effects ---------------------------------------------------------

    fn apply(&self, effects: Vec<Effect>) {
        for e in effects {
            match e {
                Effect::Send(peer, msg) => self.send(&peer, msg),
                Effect::Freeze { at } => self.host.control(Control::Freeze { x: at.x, y: at.y }),
                Effect::Unfreeze { warp_to } => self.host.control(Control::Unfreeze { x: warp_to.x, y: warp_to.y }),
                Effect::Focus(_) => self.host.state_changed(),
            }
        }
    }

    fn send(&self, peer: &PeerId, msg: Message) {
        if let Some(s) = lock(&self.peers).get(peer).and_then(|p| p.session.as_ref()) {
            let _ = s.tx.send(msg);
        }
    }

    fn broadcast(&self, msg: Message) {
        for p in lock(&self.peers).values() {
            if let Some(s) = &p.session {
                let _ = s.tx.send(msg.clone());
            }
        }
    }

    fn release_injected(&self) {
        let mut out = Vec::new();
        lock(&self.injector).release_all(&mut out);
        for o in &out {
            self.host.inject(o);
        }
    }

    // ----- arrangement -----------------------------------------------------

    /// Rebuilds the switcher layout from local screens and connected peers,
    /// giving newly paired devices a default spot (above this one).
    fn rebuild_layout(&self) {
        if self.role != Role::Primary {
            return;
        }
        let local = screens_to_rects(&lock(&self.screens));
        let connected: Vec<(PeerId, Vec<Rect>)> = lock(&self.peers)
            .iter()
            .filter(|(_, p)| p.status == Status::Connected && !p.screens.is_empty())
            .map(|(id, p)| (*id, screens_to_rects(&p.screens)))
            .collect();

        let mut layout = Layout::new(local.clone());
        {
            let mut cfg = lock(&self.config);
            let mut changed = false;
            let local_bounds = switching::bounds(&local).unwrap_or(Rect::new(0.0, 0.0, 1.0, 1.0));
            let mut occupied = local.clone();
            for peer in &cfg.trusted {
                if let (Some(p), Some(b)) = (peer.placement, switching::bounds(&records_to_rects(&peer.screens))) {
                    occupied.push(Rect::new(p.x as f64, p.y as f64, b.w, b.h));
                }
            }
            for (id, rects) in connected {
                let Some(peer) = cfg.peer_mut(&id) else { continue };
                let Some(b) = switching::bounds(&rects) else { continue };
                let placement = match peer.placement {
                    Some(p) => Point::new(p.x as f64, p.y as f64),
                    None => {
                        let p = switching::default_placement(&occupied, local_bounds, (b.w, b.h));
                        peer.placement = Some(Placement { x: p.x as f32, y: p.y as f32 });
                        occupied.push(Rect::new(p.x, p.y, b.w, b.h));
                        changed = true;
                        p
                    }
                };
                if let Some(dev) = Device::remote(id, &rects, placement) {
                    layout.devices.push(dev);
                }
            }
            if changed {
                let _ = cfg.save(&self.dir);
            }
        }
        let mut effects = Vec::new();
        lock(&self.switcher).set_layout(layout, &mut effects);
        self.apply(effects);
    }

    fn trust(&self, id: &PeerId, name: &str, platform: Platform) {
        let mut cfg = lock(&self.config);
        cfg.trust(id, name, platform);
        if let Err(e) = cfg.save(&self.dir) {
            ew_log!("could not save config: {e}");
        }
    }

    // ----- discovery -------------------------------------------------------

    fn on_discovery(&self, ev: discovery::Event) {
        if self.role != Role::Primary {
            return;
        }
        {
            let mut nearby = lock(&self.nearby);
            match ev {
                discovery::Event::Found(f) => {
                    if f.fingerprint == self.identity.id().fingerprint() {
                        return;
                    }
                    if !nearby.iter().any(|n| n.fingerprint == f.fingerprint) {
                        ew_log!("found \"{}\" at {}:{}", f.instance, f.host, f.port);
                    }
                    nearby.retain(|n| n.instance != f.instance && n.fingerprint != f.fingerprint);
                    nearby.push(f);
                }
                discovery::Event::Lost { instance } => {
                    ew_log!("lost \"{instance}\"");
                    nearby.retain(|n| n.instance != instance)
                }
            }
        }
        self.notify();
        self.host.state_changed();
    }

    // ----- secondary: listening --------------------------------------------

    fn start_listener(self: &Arc<Self>, port_override: Option<u16>) -> io::Result<u16> {
        let saved = lock(&self.config).listen_port;
        let wanted = port_override.unwrap_or(saved);
        let bind = |port: u16| -> io::Result<TcpListener> {
            if self.loopback_only {
                TcpListener::bind(("127.0.0.1", port))
            } else {
                TcpListener::bind(("::", port)).or_else(|_| TcpListener::bind(("0.0.0.0", port)))
            }
        };
        let listener = bind(wanted).or_else(|e| if wanted != 0 { bind(0) } else { Err(e) })?;
        let port = listener.local_addr()?.port();
        self.port.store(port, Ordering::SeqCst);
        if port_override.is_none() && port != saved {
            let mut cfg = lock(&self.config);
            cfg.listen_port = port;
            let _ = cfg.save(&self.dir);
        }
        let inner = self.clone();
        thread::Builder::new().name("edgeway-listener".into()).spawn(move || {
            for stream in listener.incoming() {
                if inner.shutdown.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                if inner.pending_inbound.load(Ordering::SeqCst) >= MAX_PENDING_INBOUND as u64 {
                    continue;
                }
                inner.pending_inbound.fetch_add(1, Ordering::SeqCst);
                let i = inner.clone();
                let _ = thread::Builder::new().name("edgeway-conn".into()).spawn(move || i.run_inbound(stream));
            }
        })?;
        ew_log!("listening on port {port}");
        Ok(port)
    }

    fn run_inbound(self: Arc<Self>, mut stream: TcpStream) {
        tune(&stream);
        let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
        let _ = stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT));
        let hs = noise::respond(&mut stream, &self.identity);
        let conn = match hs {
            Ok(h) => Conn { stream, sealer: h.sealer, opener: h.opener, remote: h.remote, hash: h.hash },
            Err(e) => {
                self.pending_inbound.fetch_sub(1, Ordering::SeqCst);
                ew_log!("handshake failed: {e}");
                return;
            }
        };
        let mut conn = conn;
        let trusted = lock(&self.config).is_trusted(&conn.remote);
        let ok = trusted || {
            let r = self.pair_as_responder(&mut conn);
            if let Err(e) = &r {
                ew_log!("pairing ended: {e}");
            }
            r.is_ok()
        };
        self.pending_inbound.fetch_sub(1, Ordering::SeqCst);
        if ok {
            self.establish(conn);
        } else {
            let _ = conn.stream.shutdown(Shutdown::Both);
        }
    }

    fn pair_as_responder(&self, conn: &mut Conn) -> io::Result<()> {
        let (peer, platform) = match conn.recv(PAIR_REQUEST_TIMEOUT)? {
            Message::PairRequest { name, platform } => (name, platform),
            other => return Err(protocol_error(&format!("unexpected {} before pairing", other.kind()))),
        };
        if self.paused.load(Ordering::SeqCst) {
            conn.send(&Message::PairRejected(PairReject::Denied))?;
            return Err(protocol_error("paused"));
        }
        if !lock(&self.throttle).allowed(Instant::now()) {
            conn.send(&Message::PairRejected(PairReject::TooManyAttempts))?;
            return Err(protocol_error("too many attempts"));
        }
        let code = self.fixed_code.clone().unwrap_or_else(pairing::generate_code);
        {
            let mut slot = lock(&self.pairing);
            if !matches!(*slot, PairingSlot::Idle) {
                drop(slot);
                conn.send(&Message::PairRejected(PairReject::Busy))?;
                return Err(protocol_error("busy"));
            }
            *slot = PairingSlot::Showing { peer: peer.clone(), code: code.clone(), stream: conn.stream.try_clone()? };
        }
        let result = self.responder_exchange(conn, &peer, platform, &code);
        *lock(&self.pairing) = PairingSlot::Idle;
        match &result {
            Ok(()) => self.host.pairing(PairingUi::Succeeded { peer }),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                self.host.pairing(PairingUi::Failed { peer, reason: PairReject::WrongCode.message().into() })
            }
            Err(_) => self.host.pairing(PairingUi::Closed),
        }
        self.host.state_changed();
        result
    }

    fn responder_exchange(&self, conn: &mut Conn, peer: &str, platform: Platform, code: &str) -> io::Result<()> {
        self.host.pairing(PairingUi::ShowCode { peer: peer.to_string(), code: code.to_string() });
        self.host.state_changed();
        let (state, msg_b) = pairing::responder_start(code);
        conn.send(&Message::PairChallenge { spake: msg_b })?;
        let (spake, confirm) = match conn.recv(PAIR_CODE_TIMEOUT)? {
            Message::PairResponse { spake, confirm } => (spake, confirm),
            other => return Err(protocol_error(&format!("unexpected {}", other.kind()))),
        };
        let me = self.identity.id();
        let key = state.finish(&spake);
        let verified = key
            .as_deref()
            .is_some_and(|k| pairing::verify_tag(k, Side::Initiator, &conn.hash, &conn.remote, &me, &confirm));
        let Some(key) = key.filter(|_| verified) else {
            lock(&self.throttle).record_failure(Instant::now());
            let _ = conn.send(&Message::PairRejected(PairReject::WrongCode));
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "wrong code"));
        };
        lock(&self.throttle).reset();
        let tag = pairing::confirm_tag(&key, Side::Responder, &conn.hash, &conn.remote, &me);
        conn.send(&Message::PairConfirm { confirm: tag })?;
        self.trust(&conn.remote, peer, platform);
        ew_log!("paired with {peer} ({})", conn.remote.short());
        Ok(())
    }

    fn cancel_pairing(&self) {
        let slot = std::mem::replace(&mut *lock(&self.pairing), PairingSlot::Idle);
        match slot {
            PairingSlot::Idle => return,
            PairingSlot::Showing { stream, .. } | PairingSlot::AwaitingCode { stream, .. } => {
                let _ = stream.shutdown(Shutdown::Both);
            }
            PairingSlot::Working { stream, .. } => {
                if let Some(s) = stream {
                    let _ = s.shutdown(Shutdown::Both);
                }
            }
        }
        self.host.pairing(PairingUi::Closed);
        self.host.state_changed();
    }

    // ----- primary: dialing ------------------------------------------------

    fn connect(&self, host: &str, port: u16) -> io::Result<Conn> {
        let addrs: Vec<_> = (host, port).to_socket_addrs()?.collect();
        let mut last = io::Error::new(io::ErrorKind::NotFound, format!("{host} has no address"));
        for addr in addrs {
            match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
                Ok(mut stream) => {
                    tune(&stream);
                    stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
                    stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;
                    let h = noise::initiate(&mut stream, &self.identity)?;
                    return Ok(Conn { stream, sealer: h.sealer, opener: h.opener, remote: h.remote, hash: h.hash });
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn start_pairing(self: Arc<Self>, instance: &str) -> bool {
        let Some(found) = lock(&self.nearby).iter().find(|n| n.instance == instance).cloned() else {
            return false;
        };
        {
            let mut slot = lock(&self.pairing);
            if !matches!(*slot, PairingSlot::Idle) {
                return false;
            }
            *slot = PairingSlot::Working { peer: found.instance.clone(), stream: None };
        }
        self.host.state_changed();
        let inner = self.clone();
        thread::spawn(move || {
            let peer = found.instance.clone();
            let result = inner.connect(&found.host, found.port).and_then(|mut conn| {
                inner.pair_as_initiator(&mut conn, &peer).map(|_| conn)
            });
            let cancelled = matches!(*lock(&inner.pairing), PairingSlot::Idle);
            *lock(&inner.pairing) = PairingSlot::Idle;
            match result {
                Ok(conn) => {
                    inner.host.pairing(PairingUi::Succeeded { peer });
                    inner.host.state_changed();
                    let id = conn.remote;
                    lock(&inner.peers).entry(id).or_default().status = Status::Connecting;
                    inner.establish(conn);
                    inner.after_session(&id);
                }
                Err(e) => {
                    ew_log!("pairing with {peer} failed: {e}");
                    if !cancelled {
                        let reason = if e.kind() == io::ErrorKind::PermissionDenied {
                            e.to_string()
                        } else {
                            format!("Couldn’t reach {peer}. Make sure Edgeway is open on it.")
                        };
                        inner.host.pairing(PairingUi::Failed { peer, reason });
                    }
                    inner.host.state_changed();
                }
            }
        });
        true
    }

    fn pair_as_initiator(&self, conn: &mut Conn, peer: &str) -> io::Result<()> {
        if lock(&self.config).is_trusted(&conn.remote) {
            return Ok(());
        }
        conn.send(&Message::PairRequest { name: self.name.clone(), platform: Platform::current() })?;
        let challenge = match conn.recv(PAIR_REQUEST_TIMEOUT)? {
            Message::PairChallenge { spake } => spake,
            Message::PairRejected(r) => return Err(io::Error::new(io::ErrorKind::PermissionDenied, r.message())),
            other => return Err(protocol_error(&format!("unexpected {}", other.kind()))),
        };
        let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();
        {
            let mut slot = lock(&self.pairing);
            if matches!(*slot, PairingSlot::Idle) {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            *slot = PairingSlot::AwaitingCode { peer: peer.to_string(), tx, stream: conn.stream.try_clone()? };
        }
        self.host.pairing(PairingUi::EnterCode { peer: peer.to_string() });
        self.host.state_changed();
        let code = rx
            .recv_timeout(PAIR_CODE_TIMEOUT)
            .map_err(|_| io::Error::new(io::ErrorKind::Interrupted, "cancelled"))?;
        {
            let mut slot = lock(&self.pairing);
            if matches!(*slot, PairingSlot::Idle) {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
            }
            *slot = PairingSlot::Working { peer: peer.to_string(), stream: conn.stream.try_clone().ok() };
        }
        self.host.state_changed();

        let me = self.identity.id();
        let (state, msg_a) = pairing::initiator_start(&code);
        let key = state.finish(&challenge).ok_or_else(|| protocol_error("bad SPAKE2 message"))?;
        let tag = pairing::confirm_tag(&key, Side::Initiator, &conn.hash, &me, &conn.remote);
        conn.send(&Message::PairResponse { spake: msg_a, confirm: tag })?;
        match conn.recv(PAIR_REQUEST_TIMEOUT)? {
            Message::PairConfirm { confirm }
                if pairing::verify_tag(&key, Side::Responder, &conn.hash, &me, &conn.remote, &confirm) =>
            {
                self.trust(&conn.remote, peer, Platform::Unknown);
                ew_log!("paired with {peer} ({})", conn.remote.short());
                Ok(())
            }
            Message::PairRejected(r) => Err(io::Error::new(io::ErrorKind::PermissionDenied, r.message())),
            _ => Err(io::Error::new(io::ErrorKind::PermissionDenied, PairReject::WrongCode.message())),
        }
    }

    /// Keeps trusted devices connected: dials those that are visible and not
    /// connected, with exponential backoff, and otherwise sleeps.
    fn dialer(self: Arc<Self>) {
        while !self.shutdown.load(Ordering::SeqCst) {
            let now = Instant::now();
            let mut next: Option<Instant> = None;
            let mut targets = Vec::new();
            {
                let cfg = lock(&self.config);
                let nearby = lock(&self.nearby);
                let mut peers = lock(&self.peers);
                for t in &cfg.trusted {
                    let Some(id) = t.id() else { continue };
                    let Some(found) = nearby.iter().find(|n| n.fingerprint == id.fingerprint()) else { continue };
                    let st = peers.entry(id).or_default();
                    if st.status != Status::Offline {
                        continue;
                    }
                    match st.retry_at {
                        Some(at) if at > now => next = Some(next.map_or(at, |n| n.min(at))),
                        _ => {
                            st.status = Status::Connecting;
                            targets.push((id, found.host.clone(), found.port));
                        }
                    }
                }
            }
            if !targets.is_empty() {
                self.host.state_changed();
            }
            for (id, host, port) in targets {
                let inner = self.clone();
                let _ = thread::Builder::new().name("edgeway-session".into()).spawn(move || inner.dial(id, &host, port));
            }
            self.wait(next.map(|n| n.saturating_duration_since(Instant::now()) + Duration::from_millis(5)));
        }
    }

    fn dial(self: Arc<Self>, id: PeerId, host: &str, port: u16) {
        match self.connect(host, port) {
            Ok(conn) if conn.remote == id => {
                self.establish(conn);
            }
            Ok(conn) => {
                let _ = conn.stream.shutdown(Shutdown::Both);
                if let Some(st) = lock(&self.peers).get_mut(&id) {
                    st.error = Some("This device has a new identity. Forget it and pair again.".into());
                }
            }
            Err(e) => {
                if let Some(st) = lock(&self.peers).get_mut(&id) {
                    st.error = Some(e.to_string());
                }
            }
        }
        self.after_session(&id);
    }

    /// Schedules the next attempt after a session or a failed dial.
    fn after_session(&self, id: &PeerId) {
        {
            let mut peers = lock(&self.peers);
            let Some(st) = peers.get_mut(id) else { return };
            if st.session.is_some() {
                return;
            }
            st.status = Status::Offline;
            st.retry_at = Some(Instant::now() + backoff(st.attempts));
            st.attempts = st.attempts.saturating_add(1);
        }
        self.host.state_changed();
        self.notify();
    }

    // ----- sessions ----------------------------------------------------------

    /// Exchanges `Hello`, registers the session and runs its reader loop on
    /// the current thread until the connection ends.
    fn establish(self: &Arc<Self>, mut conn: Conn) {
        let remote = conn.remote;
        let hello = Hello {
            version: PROTOCOL_VERSION,
            name: self.name.clone(),
            platform: Platform::current(),
            screens: lock(&self.screens).clone(),
        };
        if conn.send(&Message::Hello(hello)).is_err() {
            return;
        }
        let peer_hello = match conn.recv(Duration::from_secs(10)) {
            Ok(Message::Hello(h)) => h,
            _ => return,
        };
        if peer_hello.version != PROTOCOL_VERSION {
            if let Some(st) = lock(&self.peers).get_mut(&remote) {
                st.error = Some("This device runs an incompatible version of Edgeway.".into());
            }
            return;
        }
        let Ok(write_stream) = conn.stream.try_clone() else { return };
        let Ok(kill_stream) = conn.stream.try_clone() else { return };
        let _ = conn.stream.set_read_timeout(None);
        let _ = write_stream.set_write_timeout(Some(Duration::from_secs(3)));

        let id = self.next_session.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel::<Message>();
        let last_rx = Arc::new(AtomicU64::new(self.now_ms()));
        let mut replaced = Vec::new();
        {
            let mut peers = lock(&self.peers);
            if self.role == Role::Secondary {
                // One controller at a time.
                for (pid, p) in peers.iter_mut() {
                    if *pid != remote {
                        if let Some(s) = p.session.take() {
                            replaced.push(s);
                        }
                        p.status = Status::Offline;
                    }
                }
            }
            let st = peers.entry(remote).or_default();
            if let Some(old) = st.session.take() {
                replaced.push(old);
            }
            st.status = Status::Connected;
            st.session = Some(SessionHandle { id, tx, stream: kill_stream, last_rx: last_rx.clone() });
            st.name = peer_hello.name.clone();
            st.platform = Some(peer_hello.platform);
            st.screens = peer_hello.screens.clone();
            st.attempts = 0;
            st.retry_at = None;
            st.error = None;
        }
        for old in replaced {
            let _ = old.stream.shutdown(Shutdown::Both);
        }
        {
            let mut cfg = lock(&self.config);
            if let Some(p) = cfg.peer_mut(&remote) {
                p.name = peer_hello.name.clone();
                p.platform = peer_hello.platform.code();
                p.screens = peer_hello.screens.iter().map(ScreenRecord::from).collect();
                let _ = cfg.save(&self.dir);
            }
        }
        ew_log!("connected to {} ({})", peer_hello.name, remote.short());
        self.rebuild_layout();
        self.host.state_changed();
        self.notify();

        let Conn { mut stream, mut sealer, mut opener, .. } = conn;
        let _ = thread::Builder::new().name("edgeway-writer".into()).spawn(move || {
            let mut w = BufWriter::with_capacity(16 * 1024, write_stream);
            'outer: while let Ok(msg) = rx.recv() {
                let mut next = Some(msg);
                while let Some(m) = next {
                    if sealer.send(&mut w, &m).is_err() {
                        break 'outer;
                    }
                    next = rx.try_recv().ok();
                }
                if w.flush().is_err() {
                    break;
                }
            }
            let _ = w.get_ref().shutdown(Shutdown::Both);
        });

        loop {
            match opener.recv(&mut stream) {
                Ok(msg) => {
                    last_rx.store(self.now_ms(), Ordering::SeqCst);
                    if msg == Message::Bye {
                        break;
                    }
                    self.dispatch(&remote, msg);
                }
                Err(_) => break,
            }
        }
        let _ = stream.shutdown(Shutdown::Both);
        self.session_ended(&remote, id);
    }

    fn dispatch(&self, remote: &PeerId, msg: Message) {
        match msg {
            Message::Ping { t } => self.send(remote, Message::Pong { t }),
            Message::Pong { t } => {
                let rtt = self.now_us().saturating_sub(t) as f32 / 1000.0;
                if let Some(st) = lock(&self.peers).get_mut(remote) {
                    st.rtt_ms = Some(rtt);
                }
            }
            Message::Screens(screens) => {
                if let Some(st) = lock(&self.peers).get_mut(remote) {
                    st.screens = screens.clone();
                }
                {
                    let mut cfg = lock(&self.config);
                    if let Some(p) = cfg.peer_mut(remote) {
                        p.screens = screens.iter().map(ScreenRecord::from).collect();
                        let _ = cfg.save(&self.dir);
                    }
                }
                self.rebuild_layout();
                self.host.state_changed();
            }
            Message::Enter { .. }
            | Message::Leave
            | Message::MouseMove { .. }
            | Message::MouseButton { .. }
            | Message::Scroll { .. }
            | Message::Key { .. }
            | Message::ReleaseAll => {
                if self.role != Role::Secondary || self.paused.load(Ordering::SeqCst) {
                    return;
                }
                let focus_change = match msg {
                    Message::Enter { .. } => Some(Some(*remote)),
                    Message::Leave => Some(None),
                    _ => None,
                };
                let mut out = Vec::with_capacity(2);
                lock(&self.injector).apply(&msg, Instant::now(), &mut out);
                for o in &out {
                    self.host.inject(o);
                }
                if let Some(f) = focus_change {
                    *lock(&self.controlled_by) = f;
                    self.host.state_changed();
                }
            }
            _ => {}
        }
    }

    fn session_ended(&self, remote: &PeerId, id: u64) {
        let ended = {
            let mut peers = lock(&self.peers);
            match peers.get_mut(remote) {
                Some(st) if st.session.as_ref().map(|s| s.id) == Some(id) => {
                    st.session = None;
                    st.status = Status::Offline;
                    st.rtt_ms = None;
                    true
                }
                _ => false,
            }
        };
        if !ended {
            return;
        }
        ew_log!("disconnected from {}", remote.short());
        match self.role {
            Role::Secondary => {
                self.release_injected();
                let mut c = lock(&self.controlled_by);
                if c.as_ref() == Some(remote) {
                    *c = None;
                }
            }
            Role::Primary => {
                let mut effects = Vec::new();
                lock(&self.switcher).peer_lost(remote, &mut effects);
                self.apply(effects);
                self.rebuild_layout();
            }
        }
        self.host.state_changed();
        self.notify();
    }

    /// Pings every second while connected and ends silent sessions, so a
    /// vanished peer never leaves keys held down.
    fn heartbeat(self: Arc<Self>) {
        while !self.shutdown.load(Ordering::SeqCst) {
            let sessions: Vec<(Sender<Message>, TcpStream, u64)> = lock(&self.peers)
                .values()
                .filter_map(|p| p.session.as_ref())
                .filter_map(|s| s.stream.try_clone().ok().map(|st| (s.tx.clone(), st, s.last_rx.load(Ordering::SeqCst))))
                .collect();
            if sessions.is_empty() {
                self.wait(None);
                continue;
            }
            let (now, stamp) = (self.now_ms(), self.now_us());
            for (tx, stream, last) in sessions {
                if now.saturating_sub(last) > HEARTBEAT_TIMEOUT_MS {
                    ew_log!("peer stopped responding");
                    let _ = stream.shutdown(Shutdown::Both);
                } else {
                    let _ = tx.send(Message::Ping { t: stamp });
                }
            }
            self.wait(Some(HEARTBEAT_INTERVAL));
        }
    }

    // ----- snapshot ----------------------------------------------------------

    fn snapshot(&self) -> Snapshot {
        let cfg = lock(&self.config).clone();
        let nearby = lock(&self.nearby).clone();
        let focus = lock(&self.switcher).focus();
        let controlled_by = *lock(&self.controlled_by);
        let peers = lock(&self.peers);

        let devices = cfg
            .trusted
            .iter()
            .filter_map(|t| {
                let id = t.id()?;
                let st = peers.get(&id);
                let online = nearby.iter().any(|n| n.fingerprint == id.fingerprint());
                let screens = match st {
                    Some(s) if !s.screens.is_empty() => s.screens.iter().map(ScreenRecord::from).collect(),
                    _ => t.screens.clone(),
                };
                Some(DeviceSnapshot {
                    id: id.to_hex(),
                    short_id: id.short(),
                    name: st.filter(|s| !s.name.is_empty()).map(|s| s.name.clone()).unwrap_or_else(|| t.name.clone()),
                    platform: st.and_then(|s| s.platform).unwrap_or(t.platform()).label().into(),
                    status: st.map(|s| s.status).unwrap_or_default().label().into(),
                    online: online || st.is_some_and(|s| s.status == Status::Connected),
                    rtt_ms: st.and_then(|s| s.rtt_ms),
                    error: st.and_then(|s| s.error.clone()).filter(|_| st.is_some_and(|s| s.status != Status::Connected)),
                    placement: t.placement,
                    screens,
                })
            })
            .collect();

        let trusted_fps: Vec<String> = cfg.trusted.iter().filter_map(|t| t.id()).map(|id| id.fingerprint()).collect();
        let nearby = nearby
            .iter()
            .filter(|n| !trusted_fps.contains(&n.fingerprint))
            .map(|n| NearbySnapshot {
                instance: n.instance.clone(),
                fingerprint: n.fingerprint.clone(),
                platform: n.platform.label().into(),
            })
            .collect();

        let pairing = match &*lock(&self.pairing) {
            PairingSlot::Idle => None,
            PairingSlot::Showing { peer, code, .. } => {
                Some(PairingSnapshot { state: "show_code".into(), peer: peer.clone(), code: Some(code.clone()) })
            }
            PairingSlot::AwaitingCode { peer, .. } => {
                Some(PairingSnapshot { state: "enter_code".into(), peer: peer.clone(), code: None })
            }
            PairingSlot::Working { peer, .. } => {
                Some(PairingSnapshot { state: "working".into(), peer: peer.clone(), code: None })
            }
        };

        Snapshot {
            role: self.role.label().into(),
            name: self.name.clone(),
            id: self.identity.id().to_hex(),
            short_id: self.identity.id().short(),
            paused: self.paused.load(Ordering::SeqCst),
            focus: focus.map(|f| f.to_hex()),
            controlled_by: controlled_by.map(|c| c.to_hex()),
            edge_resistance: cfg.edge_resistance,
            port: self.port.load(Ordering::SeqCst),
            screens: lock(&self.screens).iter().map(ScreenRecord::from).collect(),
            devices,
            nearby,
            pairing,
        }
    }
}

fn records_to_rects(records: &[ScreenRecord]) -> Vec<Rect> {
    records.iter().map(|s| Rect::new(s.x as f64, s.y as f64, s.w as f64, s.h as f64)).collect()
}

/// Everything the UI needs, serialised to JSON for the shells.
#[derive(Clone, Debug, SerJson)]
pub struct Snapshot {
    pub role: String,
    pub name: String,
    pub id: String,
    pub short_id: String,
    pub paused: bool,
    /// Primary: device receiving input (`null` = this one).
    pub focus: Option<String>,
    /// Secondary: device currently controlling this one.
    pub controlled_by: Option<String>,
    pub edge_resistance: f32,
    pub port: u16,
    pub screens: Vec<ScreenRecord>,
    pub devices: Vec<DeviceSnapshot>,
    pub nearby: Vec<NearbySnapshot>,
    pub pairing: Option<PairingSnapshot>,
}

#[derive(Clone, Debug, SerJson)]
pub struct DeviceSnapshot {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub platform: String,
    /// `connected`, `connecting` or `offline`.
    pub status: String,
    /// Visible on the network right now.
    pub online: bool,
    pub rtt_ms: Option<f32>,
    pub error: Option<String>,
    pub placement: Option<Placement>,
    pub screens: Vec<ScreenRecord>,
}

#[derive(Clone, Debug, SerJson)]
pub struct NearbySnapshot {
    pub instance: String,
    pub fingerprint: String,
    pub platform: String,
}

#[derive(Clone, Debug, SerJson)]
pub struct PairingSnapshot {
    /// `show_code`, `enter_code` or `working`.
    pub state: String,
    pub peer: String,
    pub code: Option<String>,
}
