//! A headless Edgeway peer for development and testing.
//!
//! Runs the real engine with real DNS-SD discovery but no UI and no input
//! capture or injection: received input is printed instead. Two copies on one
//! machine (or on two machines) exercise discovery, pairing, the encrypted
//! session, latency and reconnection end to end.
//!
//!   cargo run --example peer -- secondary /tmp/ew-s --code 482913
//!   cargo run --example peer -- primary   /tmp/ew-p --code 482913 --type hello
//!
//! Add `--hold` to the primary to press a key and keep it held, then freeze
//! the process (`kill -STOP`): the secondary must release the key by itself
//! once the heartbeat times out.
//!
//! `--code` fixes the pairing code (the secondary shows it, the primary
//! types it). Never use a fixed code outside testing.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use edgeway_core::engine::{Control, Engine, EngineOptions, Host, PairingUi, Role};
use edgeway_core::injector::Output;
use edgeway_core::protocol::Screen;
use edgeway_core::switching::LocalEvent;

struct Printer;

impl Host for Printer {
    fn inject(&self, ev: &Output) {
        if *ev != Output::Wake {
            println!("inject   {ev:?}");
        }
    }
    fn control(&self, c: Control) {
        println!("control  {c:?}");
    }
    fn state_changed(&self) {}
    fn pairing(&self, ev: PairingUi) {
        println!("pairing  {ev:?}");
    }
}

fn arg(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

fn wait(what: &str, timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    eprintln!("timed out waiting for {what}");
    false
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let role = match args.get(1).map(String::as_str) {
        Some("primary") => Role::Primary,
        Some("secondary") => Role::Secondary,
        _ => {
            eprintln!("usage: peer primary|secondary <dir> [--code 123456] [--type text] [--name name]");
            std::process::exit(2);
        }
    };
    let dir = PathBuf::from(args.get(2).cloned().unwrap_or_else(|| "/tmp/edgeway-peer".into()));
    let code = arg(&args, "--code");
    let name = arg(&args, "--name").unwrap_or_else(|| format!("peer-{}", if role == Role::Primary { "primary" } else { "secondary" }));

    let mut opts = EngineOptions::new(dir, role, name);
    opts.fixed_pairing_code = code.clone();
    opts.screens = vec![Screen { x: 0.0, y: 0.0, w: 1512.0, h: 982.0, name: "Display".into() }];
    let engine = Arc::new(Engine::start(opts, Arc::new(Printer)).expect("engine starts"));
    println!("started  {:?} {}", role, engine.id().short());

    if role == Role::Secondary {
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }

    // Primary: pair with the first device found (unless already trusted).
    if !wait("a device on the network", Duration::from_secs(20), || {
        let s = engine.snapshot();
        !s.nearby.is_empty() || !s.devices.is_empty()
    }) {
        std::process::exit(1);
    }
    let snap = engine.snapshot();
    if snap.devices.is_empty() {
        let target = snap.nearby[0].instance.clone();
        println!("pairing  with {target}");
        let started = Instant::now();
        engine.pair(&target);
        wait("code prompt", Duration::from_secs(10), || engine.snapshot().pairing.is_some_and(|p| p.state == "enter_code"));
        engine.submit_pairing_code(code.as_deref().unwrap_or("000000"));
        if !wait("connection", Duration::from_secs(15), || engine.snapshot().devices.iter().any(|d| d.status == "connected")) {
            std::process::exit(1);
        }
        println!("paired   in {} ms", started.elapsed().as_millis());
    } else {
        let started = Instant::now();
        engine.network_changed();
        if !wait("reconnection", Duration::from_secs(15), || engine.snapshot().devices.iter().any(|d| d.status == "connected")) {
            std::process::exit(1);
        }
        println!("reconnected in {} ms (no pairing needed)", started.elapsed().as_millis());
    }

    // Cross into the device above and type.
    engine.set_edge_resistance(0.0);
    engine.handle_local_event(&LocalEvent::MouseMove { x: 700.0, y: 0.0, dx: 0.0, dy: -10.0 });
    if let Some(text) = arg(&args, "--type") {
        for ch in text.chars().filter(|c| c.is_ascii_lowercase()) {
            let key = 0x04 + (ch as u16 - 'a' as u16);
            engine.handle_local_event(&LocalEvent::Key { key, down: true, repeat: false });
            engine.handle_local_event(&LocalEvent::Key { key, down: false, repeat: false });
        }
    }
    if args.iter().any(|a| a == "--hold") {
        // Press a key and never release it, to test the receiver's safety net.
        engine.handle_local_event(&LocalEvent::Key { key: 0x04, down: true, repeat: false });
        println!("holding  A down; freeze or kill this process now");
        std::thread::sleep(Duration::from_secs(60));
    }
    std::thread::sleep(Duration::from_millis(2500));
    let rtts: Vec<f32> = engine.snapshot().devices.iter().filter_map(|d| d.rtt_ms).collect();
    println!("rtt      {rtts:?} ms");
    engine.stop();
}
