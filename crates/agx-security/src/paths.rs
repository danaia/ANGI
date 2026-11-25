//! Where the policy, keys and pinned peers live. Tests and previews use a private directory.
use std::path::PathBuf;

pub const DIR_ENV: &str = "AGX_SECURITY_DIR";

#[derive(Clone, Debug)]
pub struct Paths {
    /// The chosen policy, on the data volume so a rootfs replacement (OS update) keeps it.
    pub policy: PathBuf,
    /// The image's default, or the one a provisioned card carries. Used until a choice is saved.
    pub image_policy: PathBuf,
    /// Present only where `policy` lives on a mount that must be there before it is used.
    pub data_mount: Option<PathBuf>,
    /// OpenSSH private key (or `id_dropbear` where only Dropbear's tools exist).
    pub key_dir: PathBuf,
    pub host_key: PathBuf,
    pub peers: PathBuf,
    /// Graphics options the greeter hands to the session; `AGX_GUI_VNC` lives here.
    pub gui_env: PathBuf,
}

impl Paths {
    /// The data volume is mounted (or no mount is required, as in tests and previews).
    pub fn data_ready(&self) -> bool {
        match &self.data_mount {
            None => true,
            Some(mount) => std::fs::read_to_string("/proc/mounts").is_ok_and(|mounts| {
                mounts.lines().any(|line| line.split_whitespace().nth(1) == mount.to_str())
            }),
        }
    }

    pub fn system() -> Self {
        if let Some(dir) = std::env::var_os(DIR_ENV).filter(|dir| !dir.is_empty()) {
            let dir = PathBuf::from(dir);
            return Self {
                policy: dir.join("data-security.conf"),
                image_policy: dir.join("security.conf"),
                data_mount: None,
                key_dir: dir.join("ssh"),
                host_key: dir.join("host_key"),
                peers: dir.join("peer_hosts"),
                gui_env: dir.join("gui.env"),
            };
        }
        Self {
            policy: "/var/lib/agx-data/security.conf".into(),
            image_policy: "/etc/agx/security.conf".into(),
            data_mount: Some("/var/lib/agx-data".into()),
            key_dir: "/root/.ssh".into(),
            host_key: "/etc/dropbear/dropbear_ed25519_host_key".into(),
            peers: "/etc/agx/peer_hosts".into(),
            gui_env: "/etc/agx/gui.env".into(),
        }
    }
}
