//! Settings and trusted devices, stored as small human-readable JSON.
//!
//! The file lives next to `identity.key` (on macOS:
//! `~/Library/Application Support/Edgeway/config.json`) and is written
//! atomically so a crash can never leave it half-written.

use nanoserde::{DeJson, SerJson};
use std::fs;
use std::io;
use std::path::Path;

use crate::identity::PeerId;
use crate::protocol::{Platform, Screen};

const FILE: &str = "config.json";
const CURRENT_VERSION: u32 = 1;

/// Default push past the edge, in points, before control moves to another
/// device. Enough to reach the menu bar without slipping away by accident.
pub const DEFAULT_EDGE_RESISTANCE: f32 = 40.0;

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct Config {
    #[nserde(default)]
    pub version: u32,
    /// TCP port the receiving side listens on. 0 picks one on first run.
    #[nserde(default)]
    pub listen_port: u16,
    #[nserde(default = "40.0")]
    pub edge_resistance: f32,
    #[nserde(default)]
    pub trusted: Vec<TrustedPeer>,
}

impl Default for Config {
    fn default() -> Self {
        Config { version: CURRENT_VERSION, listen_port: 0, edge_resistance: DEFAULT_EDGE_RESISTANCE, trusted: Vec::new() }
    }
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct TrustedPeer {
    /// Hex-encoded public key.
    pub key: String,
    pub name: String,
    #[nserde(default)]
    pub platform: u8,
    /// Where the device's displays sit in the arrangement, in the controlling
    /// Mac's coordinate space (top-left of the device's bounding box).
    #[nserde(default)]
    pub placement: Option<Placement>,
    /// Last known displays, so the arrangement can be shown while offline.
    #[nserde(default)]
    pub screens: Vec<ScreenRecord>,
    /// Seconds since the Unix epoch.
    #[nserde(default)]
    pub paired_at: u64,
}

impl TrustedPeer {
    pub fn id(&self) -> Option<PeerId> {
        PeerId::from_hex(&self.key)
    }

    pub fn platform(&self) -> Platform {
        Platform::from_code(self.platform)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, SerJson, DeJson)]
pub struct Placement {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct ScreenRecord {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[nserde(default)]
    pub name: String,
}

impl From<&Screen> for ScreenRecord {
    fn from(s: &Screen) -> Self {
        ScreenRecord { x: s.x, y: s.y, w: s.w, h: s.h, name: s.name.clone() }
    }
}

impl From<&ScreenRecord> for Screen {
    fn from(s: &ScreenRecord) -> Self {
        Screen { x: s.x, y: s.y, w: s.w, h: s.h, name: s.name.clone() }
    }
}

impl Config {
    /// Loads the configuration. A missing file yields defaults; an unreadable
    /// one is kept aside as `config.json.broken` and replaced by defaults.
    pub fn load(dir: &Path) -> Config {
        let path = dir.join(FILE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => return Config::default(),
        };
        match Config::deserialize_json(&text) {
            Ok(mut cfg) => {
                cfg.version = CURRENT_VERSION;
                cfg.trusted.retain(|p| p.id().is_some());
                if !cfg.edge_resistance.is_finite() || cfg.edge_resistance < 0.0 {
                    cfg.edge_resistance = DEFAULT_EDGE_RESISTANCE;
                }
                cfg
            }
            Err(e) => {
                ew_log!("config.json could not be read ({e}); starting fresh");
                let _ = fs::rename(&path, dir.join("config.json.broken"));
                Config::default()
            }
        }
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let tmp = dir.join(".config.json.tmp");
        fs::write(&tmp, self.serialize_json())?;
        fs::rename(tmp, dir.join(FILE))
    }

    pub fn peer(&self, id: &PeerId) -> Option<&TrustedPeer> {
        let key = id.to_hex();
        self.trusted.iter().find(|p| p.key == key)
    }

    pub fn peer_mut(&mut self, id: &PeerId) -> Option<&mut TrustedPeer> {
        let key = id.to_hex();
        self.trusted.iter_mut().find(|p| p.key == key)
    }

    pub fn is_trusted(&self, id: &PeerId) -> bool {
        self.peer(id).is_some()
    }

    /// Adds or refreshes a trusted device.
    pub fn trust(&mut self, id: &PeerId, name: &str, platform: Platform) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Some(p) = self.peer_mut(id) {
            p.name = name.to_string();
            p.platform = platform.code();
            p.paired_at = now;
        } else {
            self.trusted.push(TrustedPeer {
                key: id.to_hex(),
                name: name.to_string(),
                platform: platform.code(),
                placement: None,
                screens: Vec::new(),
                paired_at: now,
            });
        }
    }

    pub fn forget(&mut self, id: &PeerId) -> bool {
        let key = id.to_hex();
        let before = self.trusted.len();
        self.trusted.retain(|p| p.key != key);
        before != self.trusted.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Identity;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("edgeway-cfg-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn saves_and_loads_trusted_devices() {
        let dir = temp_dir("roundtrip");
        let id = Identity::generate().unwrap().id();
        let mut cfg = Config::default();
        cfg.trust(&id, "Mac mini", Platform::MacOS);
        cfg.peer_mut(&id).unwrap().placement = Some(Placement { x: 10.0, y: -1080.0 });
        cfg.save(&dir).unwrap();

        let loaded = Config::load(&dir);
        assert_eq!(loaded, cfg);
        assert!(loaded.is_trusted(&id));
        assert_eq!(loaded.peer(&id).unwrap().platform(), Platform::MacOS);

        let mut cfg = loaded;
        assert!(cfg.forget(&id));
        assert!(!cfg.is_trusted(&id));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tolerates_missing_fields_and_corrupt_files() {
        let dir = temp_dir("compat");
        fs::write(dir.join(FILE), "{\"trusted\":[]}").unwrap();
        let cfg = Config::load(&dir);
        assert_eq!(cfg.edge_resistance, DEFAULT_EDGE_RESISTANCE);

        fs::write(dir.join(FILE), "{ not json").unwrap();
        assert_eq!(Config::load(&dir), Config::default());
        assert!(dir.join("config.json.broken").exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
