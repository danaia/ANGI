//! Which wired links let SSH in. Stored as two `key=open|closed` lines.
use crate::{Kind, paths::Paths};
use std::{
    fs,
    io::{self, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub ethernet: bool,
    pub usb: bool,
    /// A policy file exists. Systems that predate this setting keep wired SSH open.
    pub configured: bool,
}

impl Policy {
    pub const LEGACY: Policy = Policy { ethernet: true, usb: true, configured: false };
    pub const CLOSED: Policy = Policy { ethernet: false, usb: false, configured: true };

    pub fn parse(text: &str) -> Option<Self> {
        let mut policy = Policy::CLOSED;
        let (mut ethernet, mut usb) = (false, false);
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let (key, value) = line.split_once('=')?;
            let open = match value {
                "open" => true,
                "closed" => false,
                _ => return None,
            };
            match key {
                "ethernet" if !ethernet => (policy.ethernet, ethernet) = (open, true),
                "usb" if !usb => (policy.usb, usb) = (open, true),
                _ => return None,
            }
        }
        (ethernet && usb).then_some(policy)
    }

    pub fn encode(self) -> String {
        let word = |open| if open { "open" } else { "closed" };
        format!("ethernet={}\nusb={}\n", word(self.ethernet), word(self.usb))
    }

    fn read(path: &Path) -> Option<Self> {
        match fs::read_to_string(path) {
            Ok(text) => Some(Self::parse(&text).unwrap_or(Policy::CLOSED)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(_) => Some(Policy::CLOSED),
        }
    }

    /// The saved choice, else the image's default. With neither, systems that predate this
    /// setting keep wired SSH open; a damaged or unreadable file fails closed.
    pub fn load(paths: &Paths) -> Self {
        let saved = if paths.data_ready() { Self::read(&paths.policy) } else { None };
        saved.or_else(|| Self::read(&paths.image_policy)).unwrap_or(Policy::LEGACY)
    }

    /// Keep the effective policy on the data volume once it is mounted, so replacing the root
    /// filesystem later cannot change who may connect. Does nothing once a choice is saved.
    pub fn adopt(paths: &Paths) {
        if paths.data_ready() && !paths.policy.exists() {
            let effective = Self::load(paths);
            let effective = Policy { configured: true, ..effective };
            let _ = effective.save(paths);
        }
    }

    pub fn allows(self, kind: Kind) -> bool {
        match kind {
            Kind::Ethernet => self.ethernet,
            Kind::Usb => self.usb,
            _ => true,
        }
    }

    pub fn with(mut self, kind: Kind, open: bool) -> Self {
        match kind {
            Kind::Ethernet => self.ethernet = open,
            Kind::Usb => self.usb = open,
            _ => {}
        }
        self.configured = true;
        self
    }

    pub fn save(self, paths: &Paths) -> io::Result<()> {
        let target = if paths.data_ready() { &paths.policy } else { &paths.image_policy };
        atomic_write(target, self.encode().as_bytes(), 0o644)
    }
}

/// Write beside the target and rename, so a power cut leaves the old or the new file.
pub fn atomic_write(path: &Path, contents: &[u8], mode: u32) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| io::Error::other("No directory"))?;
    fs::create_dir_all(parent)?;
    let pending = path.with_extension("new");
    let _ = fs::remove_file(&pending);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&pending)?;
    file.write_all(contents)?;
    file.sync_all()?;
    fs::set_permissions(&pending, fs::Permissions::from_mode(mode))?;
    fs::rename(&pending, path)
}
