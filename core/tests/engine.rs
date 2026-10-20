//! End-to-end tests: two engines on loopback pair, connect, route input,
//! survive disconnects and reconnect — the same lifecycle as a MacBook and a
//! Mac mini, minus mDNS.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use edgeway_core::discovery::Found;
use edgeway_core::engine::{Control, Engine, EngineOptions, Host, PairingUi, Role};
use edgeway_core::identity::Identity;
use edgeway_core::injector::Output;
use edgeway_core::protocol::{Message, Platform, Screen};
use edgeway_core::switching::{LocalEvent, Verdict};

#[derive(Debug, Clone, PartialEq)]
enum Seen {
    Inject(Output),
    Control(Control),
    Pairing(PairingUi),
}

struct TestHost {
    tx: Mutex<Sender<Seen>>,
}

impl Host for TestHost {
    fn inject(&self, ev: &Output) {
        if *ev != Output::Wake {
            let _ = self.tx.lock().unwrap().send(Seen::Inject(*ev));
        }
    }
    fn control(&self, c: Control) {
        let _ = self.tx.lock().unwrap().send(Seen::Control(c));
    }
    fn state_changed(&self) {}
    fn pairing(&self, ev: PairingUi) {
        let _ = self.tx.lock().unwrap().send(Seen::Pairing(ev));
    }
}

fn host() -> (Arc<TestHost>, Receiver<Seen>) {
    let (tx, rx) = mpsc::channel();
    (Arc::new(TestHost { tx: Mutex::new(tx) }), rx)
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("edgeway-it-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn screen(x: f32, y: f32, w: f32, h: f32) -> Screen {
    Screen { x, y, w, h, name: "Display".into() }
}

fn start(role: Role, path: &Path, name: &str) -> (Engine, Receiver<Seen>) {
    let (h, rx) = host();
    let mut opts = EngineOptions::new(path.to_path_buf(), role, name.into());
    opts.discovery = false;
    opts.loopback_only = true;
    opts.port = Some(0);
    opts.fixed_pairing_code = Some("482913".into());
    opts.screens = match role {
        Role::Primary => vec![screen(0.0, 0.0, 1512.0, 982.0)],
        Role::Secondary => vec![screen(0.0, 0.0, 2560.0, 1440.0)],
    };
    (Engine::start(opts, h).unwrap(), rx)
}

fn expect(rx: &Receiver<Seen>, what: &str, mut pred: impl FnMut(&Seen) -> bool) -> Seen {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(s) = rx.recv_timeout(Duration::from_millis(100)) {
            if pred(&s) {
                return s;
            }
        }
    }
    panic!("timed out waiting for {what}");
}

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if cond() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {what}");
}

fn announce(primary: &Engine, secondary: &Engine) {
    primary.discovered(Found {
        instance: "Mac mini".into(),
        fingerprint: secondary.id().fingerprint(),
        platform: Platform::MacOS,
        host: "127.0.0.1".into(),
        port: secondary.port(),
    });
}

fn connected(e: &Engine) -> bool {
    e.snapshot().devices.iter().any(|d| d.status == "connected")
}

fn pair(primary: &Engine, p_rx: &Receiver<Seen>, s_rx: &Receiver<Seen>, code: &str) {
    assert!(primary.pair("Mac mini"));
    expect(
        s_rx,
        "code on secondary",
        |s| matches!(s, Seen::Pairing(PairingUi::ShowCode { code, .. }) if code == "482913"),
    );
    expect(p_rx, "code prompt on primary", |s| matches!(s, Seen::Pairing(PairingUi::EnterCode { .. })));
    assert!(primary.submit_pairing_code(code));
}

fn cross_up(primary: &Engine) {
    primary.set_edge_resistance(0.0);
    let v = primary.handle_local_event(&LocalEvent::MouseMove { x: 700.0, y: 0.0, dx: 0.0, dy: -10.0 });
    assert_eq!(v, Verdict::Swallow);
}

#[test]
fn pairs_connects_routes_input_and_releases_keys_on_disconnect() {
    let (pd, sd) = (dir("p1"), dir("s1"));
    let (secondary, s_rx) = start(Role::Secondary, &sd, "Mac mini");
    let (primary, p_rx) = start(Role::Primary, &pd, "MacBook Pro");
    announce(&primary, &secondary);
    pair(&primary, &p_rx, &s_rx, "482 913");
    expect(&p_rx, "success on primary", |s| matches!(s, Seen::Pairing(PairingUi::Succeeded { .. })));
    expect(&s_rx, "success on secondary", |s| matches!(s, Seen::Pairing(PairingUi::Succeeded { .. })));
    wait_until("both connected", || connected(&primary) && connected(&secondary));

    // The Mac mini was placed above the MacBook automatically.
    let snap = primary.snapshot();
    let placement = snap.devices[0].placement.expect("default placement");
    assert_eq!((placement.x, placement.y), (-524.0, -1440.0));

    // Push through the top edge: the local cursor freezes, the mini gets the pointer.
    cross_up(&primary);
    expect(&p_rx, "freeze", |s| matches!(s, Seen::Control(Control::Freeze { .. })));
    expect(&s_rx, "pointer enters", |s| matches!(s, Seen::Inject(Output::MouseMove { x, .. }) if *x == 1224.0));
    assert_eq!(primary.focus(), Some(secondary.id()));

    // Typing goes to the mini, never to the MacBook.
    let down = LocalEvent::Key { key: 0x04, down: true, repeat: false };
    assert_eq!(primary.handle_local_event(&down), Verdict::Swallow);
    expect(&s_rx, "key down", |s| matches!(s, Seen::Inject(Output::Key { key: 0x04, down: true, .. })));

    // The MacBook disappears while A is held: the mini releases it by itself.
    drop(primary);
    expect(&s_rx, "automatic release", |s| matches!(s, Seen::Inject(Output::Key { key: 0x04, down: false, .. })));
    wait_until("secondary sees disconnect", || !connected(&secondary));

    // A "restart" of the MacBook reconnects without pairing again.
    let (primary, p_rx) = start(Role::Primary, &pd, "MacBook Pro");
    announce(&primary, &secondary);
    wait_until("automatic reconnect", || connected(&primary));
    assert!(p_rx.try_iter().all(|s| !matches!(s, Seen::Pairing(_))));

    // Losing the mini while it has focus brings the cursor home.
    cross_up(&primary);
    expect(&p_rx, "freeze", |s| matches!(s, Seen::Control(Control::Freeze { .. })));
    drop(secondary);
    expect(&p_rx, "unfreeze after loss", |s| matches!(s, Seen::Control(Control::Unfreeze { .. })));
    assert_eq!(primary.focus(), None);
    let _ = std::fs::remove_dir_all(pd);
    let _ = std::fs::remove_dir_all(sd);
}

#[test]
fn wrong_code_is_rejected_and_nothing_is_trusted() {
    let (pd, sd) = (dir("p2"), dir("s2"));
    let (secondary, s_rx) = start(Role::Secondary, &sd, "Mac mini");
    let (primary, p_rx) = start(Role::Primary, &pd, "MacBook Pro");
    announce(&primary, &secondary);
    pair(&primary, &p_rx, &s_rx, "000000");
    expect(&p_rx, "failure on primary", |s| matches!(s, Seen::Pairing(PairingUi::Failed { .. })));
    expect(&s_rx, "failure on secondary", |s| matches!(s, Seen::Pairing(PairingUi::Failed { .. })));
    assert!(primary.snapshot().devices.is_empty());
    assert!(secondary.snapshot().devices.is_empty());
    let _ = std::fs::remove_dir_all(pd);
    let _ = std::fs::remove_dir_all(sd);
}

#[test]
fn an_unpaired_device_can_never_inject_input() {
    let sd = dir("s3");
    let (secondary, s_rx) = start(Role::Secondary, &sd, "Mac mini");

    // An attacker completes a valid Noise handshake with its own key, then
    // skips pairing and sends input straight away.
    let mut stream = TcpStream::connect(("127.0.0.1", secondary.port())).unwrap();
    let attacker = Identity::generate().unwrap();
    let mut hs = edgeway_core::noise::initiate(&mut stream, &attacker).unwrap();
    for msg in [Message::Enter { x: 1.0, y: 1.0 }, Message::Key { key: 0x04, down: true, repeat: false }] {
        let _ = hs.sealer.send(&mut stream, &msg);
    }
    let _ = stream.flush();

    // The connection is closed and nothing was injected.
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut buf = [0u8; 64];
    let closed = matches!(stream.read(&mut buf), Ok(0) | Err(_));
    assert!(closed);
    std::thread::sleep(Duration::from_millis(200));
    assert!(s_rx.try_iter().all(|s| !matches!(s, Seen::Inject(_))));
    assert!(secondary.snapshot().devices.is_empty());
    let _ = std::fs::remove_dir_all(sd);
}
