//! Finding devices on the local network with DNS-SD (Bonjour / mDNS).
//!
//! Devices that accept control advertise `_edgeway._tcp` with a TXT record of
//! `v` (protocol version), `id` (key fingerprint) and `p` (platform). The
//! controlling device browses for them. Nothing here is trusted: the
//! fingerprint only lets the browser recognise a paired device early, and the
//! Noise handshake proves the real identity.
//!
//! macOS uses the system `dns_sd` API, so the OS daemon does the multicast
//! work, honours the Local Network privacy setting and costs nothing while
//! idle. The same API is available on Windows (Bonjour) and Linux (Avahi's
//! compatibility library); those backends arrive with their shells.

use crate::protocol::Platform;

/// A device seen on the network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// DNS-SD instance name (unique on the network, usually the device name).
    pub instance: String,
    pub fingerprint: String,
    pub platform: Platform,
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Found(Found),
    Lost { instance: String },
}

/// What to advertise.
pub struct Advert {
    pub name: String,
    pub port: u16,
    pub fingerprint: String,
}

/// Builds a DNS-SD TXT record: a sequence of length-prefixed `key=value`.
pub fn txt_record(pairs: &[(&str, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (k, v) in pairs {
        let entry = format!("{k}={v}");
        let bytes = &entry.as_bytes()[..entry.len().min(255)];
        out.push(bytes.len() as u8);
        out.extend_from_slice(bytes);
    }
    out
}

/// Reads one key from a TXT record.
pub fn txt_value(record: &[u8], key: &str) -> Option<String> {
    let mut i = 0;
    while i < record.len() {
        let len = record[i] as usize;
        let entry = record.get(i + 1..i + 1 + len)?;
        i += 1 + len;
        let text = std::str::from_utf8(entry).ok()?;
        if let Some((k, v)) = text.split_once('=') {
            if k.eq_ignore_ascii_case(key) {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn platform_from_label(label: Option<&str>) -> Platform {
    match label {
        Some("macos") => Platform::MacOS,
        Some("windows") => Platform::Windows,
        Some("linux") => Platform::Linux,
        _ => Platform::Unknown,
    }
}

#[cfg(target_os = "macos")]
pub use mac::Discovery;

#[cfg(not(target_os = "macos"))]
pub use fallback::Discovery;

#[cfg(not(target_os = "macos"))]
mod fallback {
    use super::{Advert, Event};
    use std::io;

    /// Placeholder until the Windows and Linux shells add their backends.
    pub struct Discovery;

    impl Discovery {
        pub fn start(_advert: Option<Advert>, _browse: bool, _sink: Box<dyn Fn(Event) + Send>) -> io::Result<Self> {
            Ok(Discovery)
        }

        pub fn stop(&mut self) {}
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{platform_from_label, txt_record, txt_value, Advert, Event, Found};
    use crate::protocol::{Platform, PROTOCOL_VERSION, SERVICE_TYPE};
    use std::collections::HashMap;
    use std::ffi::{c_char, c_int, c_uchar, c_void, CStr, CString};
    use std::io;
    use std::ptr;
    use std::thread::JoinHandle;

    type DNSServiceRef = *mut c_void;
    const FLAGS_ADD: u32 = 0x2;

    type RegisterReply =
        extern "C" fn(DNSServiceRef, u32, i32, *const c_char, *const c_char, *const c_char, *mut c_void);
    type BrowseReply =
        extern "C" fn(DNSServiceRef, u32, u32, i32, *const c_char, *const c_char, *const c_char, *mut c_void);
    type ResolveReply = extern "C" fn(
        DNSServiceRef,
        u32,
        u32,
        i32,
        *const c_char,
        *const c_char,
        u16,
        u16,
        *const c_uchar,
        *mut c_void,
    );

    extern "C" {
        fn DNSServiceRegister(
            sd: *mut DNSServiceRef,
            flags: u32,
            interface: u32,
            name: *const c_char,
            regtype: *const c_char,
            domain: *const c_char,
            host: *const c_char,
            port: u16,
            txt_len: u16,
            txt: *const c_void,
            callback: Option<RegisterReply>,
            context: *mut c_void,
        ) -> i32;
        fn DNSServiceBrowse(
            sd: *mut DNSServiceRef,
            flags: u32,
            interface: u32,
            regtype: *const c_char,
            domain: *const c_char,
            callback: BrowseReply,
            context: *mut c_void,
        ) -> i32;
        fn DNSServiceResolve(
            sd: *mut DNSServiceRef,
            flags: u32,
            interface: u32,
            name: *const c_char,
            regtype: *const c_char,
            domain: *const c_char,
            callback: ResolveReply,
            context: *mut c_void,
        ) -> i32;
        fn DNSServiceRefSockFD(sd: DNSServiceRef) -> c_int;
        fn DNSServiceProcessResult(sd: DNSServiceRef) -> i32;
        fn DNSServiceRefDeallocate(sd: DNSServiceRef);
    }

    /// Work recorded by the C callbacks, handled after each poll.
    enum Pending {
        Resolve { interface: u32, name: CString, regtype: CString, domain: CString },
        Removed { interface: u32, name: String },
        Resolved { sd: usize, host: String, port: u16, txt: Vec<u8> },
    }

    struct Context {
        pending: Vec<Pending>,
    }

    struct Op {
        sd: DNSServiceRef,
        /// For one-shot resolves: the instance being resolved.
        resolving: Option<String>,
    }

    /// Runs DNS-SD on a background thread that sleeps in `poll` until the
    /// daemon has news, so it costs no CPU while idle.
    pub struct Discovery {
        wake_fd: c_int,
        thread: Option<JoinHandle<()>>,
    }

    fn cstr(p: *const c_char) -> String {
        if p.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    }

    extern "C" fn on_register(
        _sd: DNSServiceRef,
        _flags: u32,
        err: i32,
        name: *const c_char,
        _regtype: *const c_char,
        _domain: *const c_char,
        _ctx: *mut c_void,
    ) {
        if err == 0 {
            ew_log!("advertising as \"{}\"", cstr(name));
        } else {
            ew_log!("advertising failed ({err})");
        }
    }

    extern "C" fn on_browse(
        _sd: DNSServiceRef,
        flags: u32,
        interface: u32,
        err: i32,
        name: *const c_char,
        regtype: *const c_char,
        domain: *const c_char,
        ctx: *mut c_void,
    ) {
        if err != 0 || ctx.is_null() || name.is_null() || regtype.is_null() || domain.is_null() {
            return;
        }
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        if flags & FLAGS_ADD != 0 {
            let owned = |p: *const c_char| unsafe { CStr::from_ptr(p) }.to_owned();
            ctx.pending.push(Pending::Resolve {
                interface,
                name: owned(name),
                regtype: owned(regtype),
                domain: owned(domain),
            });
        } else {
            ctx.pending.push(Pending::Removed { interface, name: cstr(name) });
        }
    }

    extern "C" fn on_resolve(
        sd: DNSServiceRef,
        _flags: u32,
        _interface: u32,
        err: i32,
        _fullname: *const c_char,
        host: *const c_char,
        port_be: u16,
        txt_len: u16,
        txt: *const c_uchar,
        ctx: *mut c_void,
    ) {
        if err != 0 || ctx.is_null() {
            return;
        }
        let ctx = unsafe { &mut *(ctx as *mut Context) };
        let txt = if txt.is_null() {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(txt, txt_len as usize) }.to_vec()
        };
        ctx.pending.push(Pending::Resolved {
            sd: sd as usize,
            host: cstr(host).trim_end_matches('.').to_string(),
            port: u16::from_be(port_be),
            txt,
        });
    }

    impl Discovery {
        pub fn start(advert: Option<Advert>, browse: bool, sink: Box<dyn Fn(Event) + Send>) -> io::Result<Self> {
            let mut fds = [0 as c_int; 2];
            if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
                return Err(io::Error::last_os_error());
            }
            let (read_fd, write_fd) = (fds[0], fds[1]);
            let thread = std::thread::Builder::new()
                .name("edgeway-discovery".into())
                .spawn(move || run(read_fd, advert, browse, sink))?;
            Ok(Discovery { wake_fd: write_fd, thread: Some(thread) })
        }

        pub fn stop(&mut self) {
            if let Some(t) = self.thread.take() {
                unsafe { libc::write(self.wake_fd, [1u8].as_ptr() as *const c_void, 1) };
                let _ = t.join();
                unsafe { libc::close(self.wake_fd) };
            }
        }
    }

    impl Drop for Discovery {
        fn drop(&mut self) {
            self.stop();
        }
    }

    fn register(advert: &Advert, regtype: &CString) -> Option<DNSServiceRef> {
        let name = CString::new(advert.name.replace('\0', "")).unwrap_or_default();
        let version = PROTOCOL_VERSION.to_string();
        let txt = txt_record(&[("v", &version), ("id", &advert.fingerprint), ("p", Platform::current().label())]);
        let mut sd: DNSServiceRef = ptr::null_mut();
        let err = unsafe {
            DNSServiceRegister(
                &mut sd,
                0,
                0,
                name.as_ptr(),
                regtype.as_ptr(),
                ptr::null(),
                ptr::null(),
                advert.port.to_be(),
                txt.len() as u16,
                txt.as_ptr() as *const c_void,
                Some(on_register),
                ptr::null_mut(),
            )
        };
        if err == 0 {
            Some(sd)
        } else {
            ew_log!("DNSServiceRegister failed ({err})");
            None
        }
    }

    fn run(wake_fd: c_int, advert: Option<Advert>, browse: bool, sink: Box<dyn Fn(Event) + Send>) {
        let mut ctx = Box::new(Context { pending: Vec::new() });
        let ctx_ptr = &mut *ctx as *mut Context as *mut c_void;
        let regtype = CString::new(SERVICE_TYPE).unwrap();
        let mut ops: Vec<Op> = Vec::new();

        if let Some(sd) = advert.as_ref().and_then(|a| register(a, &regtype)) {
            ops.push(Op { sd, resolving: None });
        }
        if browse {
            let mut sd: DNSServiceRef = ptr::null_mut();
            let err = unsafe { DNSServiceBrowse(&mut sd, 0, 0, regtype.as_ptr(), ptr::null(), on_browse, ctx_ptr) };
            if err == 0 {
                ops.push(Op { sd, resolving: None });
            } else {
                ew_log!("DNSServiceBrowse failed ({err})");
            }
        }

        // Instance name → interfaces it is currently visible on.
        let mut seen: HashMap<String, Vec<u32>> = HashMap::new();

        loop {
            let mut pollfds: Vec<libc::pollfd> = Vec::with_capacity(ops.len() + 1);
            pollfds.push(libc::pollfd { fd: wake_fd, events: libc::POLLIN, revents: 0 });
            for op in &ops {
                pollfds.push(libc::pollfd { fd: unsafe { DNSServiceRefSockFD(op.sd) }, events: libc::POLLIN, revents: 0 });
            }
            let rc = unsafe { libc::poll(pollfds.as_mut_ptr(), pollfds.len() as libc::nfds_t, -1) };
            if rc < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                break;
            }
            if pollfds[0].revents != 0 {
                break;
            }

            let mut failed = Vec::new();
            for (i, pfd) in pollfds.iter().enumerate().skip(1) {
                if pfd.revents != 0 && unsafe { DNSServiceProcessResult(ops[i - 1].sd) } != 0 {
                    failed.push(ops[i - 1].sd as usize);
                }
            }

            // A resolve is one-shot: once it has answered, it is released.
            let answered: Vec<usize> = ctx
                .pending
                .iter()
                .filter_map(|p| if let Pending::Resolved { sd, .. } = p { Some(*sd) } else { None })
                .collect();
            let mut resolved_names: HashMap<usize, String> = HashMap::new();
            ops.retain(|op| {
                let key = op.sd as usize;
                let done = failed.contains(&key) || (op.resolving.is_some() && answered.contains(&key));
                if done {
                    if let Some(name) = &op.resolving {
                        resolved_names.insert(key, name.clone());
                    }
                    unsafe { DNSServiceRefDeallocate(op.sd) };
                }
                !done
            });

            for p in std::mem::take(&mut ctx.pending) {
                match p {
                    Pending::Resolve { interface, name, regtype, domain } => {
                        let instance = name.to_string_lossy().into_owned();
                        let ifaces = seen.entry(instance.clone()).or_default();
                        if !ifaces.contains(&interface) {
                            ifaces.push(interface);
                        }
                        let mut sd: DNSServiceRef = ptr::null_mut();
                        let err = unsafe {
                            DNSServiceResolve(
                                &mut sd,
                                0,
                                interface,
                                name.as_ptr(),
                                regtype.as_ptr(),
                                domain.as_ptr(),
                                on_resolve,
                                ctx_ptr,
                            )
                        };
                        if err == 0 {
                            ops.push(Op { sd, resolving: Some(instance) });
                        }
                    }
                    Pending::Removed { interface, name } => {
                        if let Some(ifaces) = seen.get_mut(&name) {
                            ifaces.retain(|i| *i != interface);
                            if ifaces.is_empty() {
                                seen.remove(&name);
                                sink(Event::Lost { instance: name });
                            }
                        }
                    }
                    Pending::Resolved { sd, host, port, txt } => {
                        let Some(instance) = resolved_names.remove(&sd) else { continue };
                        let Some(fingerprint) = txt_value(&txt, "id") else { continue };
                        let platform = platform_from_label(txt_value(&txt, "p").as_deref());
                        sink(Event::Found(Found { instance, fingerprint, platform, host, port }));
                    }
                }
            }
        }

        for op in ops {
            unsafe { DNSServiceRefDeallocate(op.sd) };
        }
        unsafe { libc::close(wake_fd) };
        drop(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txt_records_round_trip() {
        let rec = txt_record(&[("v", "1"), ("id", "0123456789abcdef"), ("p", "macos")]);
        assert_eq!(txt_value(&rec, "id").as_deref(), Some("0123456789abcdef"));
        assert_eq!(txt_value(&rec, "P").as_deref(), Some("macos"));
        assert_eq!(txt_value(&rec, "missing"), None);
        assert_eq!(txt_value(&[200, b'a'], "a"), None);
        assert_eq!(platform_from_label(Some("linux")), Platform::Linux);
    }
}
