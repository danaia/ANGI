//! Files eligible for personal-space cleanup; never elevate desktop permissions.
use super::category::Category;
use std::{
    ffi::CString,
    fs,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
};

pub(super) const MIN_BYTES: u64 = 1024 * 1024;

#[derive(Clone)]
pub(super) struct Policy {
    pub uid: u32,
    pub roots: Vec<PathBuf>,
}

impl Policy {
    pub fn current() -> Self {
        let uid = unsafe { libc::geteuid() };
        let mut roots = Vec::new();
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            if home.starts_with("/home") && home != Path::new("/home") {
                roots.push(home);
            }
        }
        // The appliance keeps per-account data and development files on NVMe.
        roots.push("/var/lib/agx-data/angi-dev".into());
        // Web saves downloads here; ~/Downloads links to it on the appliance.
        roots.push(Path::new("/var/lib/agx-data/agx-web-users").join(uid.to_string()).join("downloads"));
        if let Some(name) = std::env::var_os("USER") {
            let name = PathBuf::from(name);
            if name.components().count() == 1 && name.file_name().is_some() {
                roots.push(Path::new("/var/lib/agx-data/users").join(name));
            }
        }
        Self { uid, roots }
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.roots.iter().any(|root| {
            path.strip_prefix(root).is_ok_and(|relative| {
                !relative.components().any(|part| match part {
                    std::path::Component::Normal(name) => {
                        name.as_bytes().starts_with(b".angi-delete-")
                            || name.as_bytes().starts_with(b".Trash-")
                            || [
                                ".git",
                                ".trash",
                                ".ssh",
                                ".gnupg",
                                ".config",
                                ".codex",
                                ".angi-agent",
                                ".cache",
                                "cache",
                                "caches",
                                "code cache",
                                "gpucache",
                                "shadercache",
                                ".npm",
                                ".ccache",
                                "tmp",
                                "temp",
                                "target",
                                "node_modules",
                                ".cargo",
                                ".rustup",
                                ".mozilla",
                                ".steam",
                                ".wine",
                                ".var",
                                "__pycache__",
                            ]
                            .iter()
                            .any(|blocked| name.as_bytes().eq_ignore_ascii_case(blocked.as_bytes()))
                    }
                    std::path::Component::ParentDir => true,
                    _ => false,
                }) && !relative.starts_with(".local/bin")
                    && !relative.starts_with(".local/share")
                    && !relative.starts_with(".local/state")
            })
        })
    }

    pub fn eligible(&self, path: &Path, metadata: &fs::Metadata) -> bool {
        metadata.is_file()
            && metadata.uid() == self.uid
            && (metadata.len() >= MIN_BYTES || Category::in_downloads(path))
            && self.contains(path)
            && path.parent().is_some_and(writable_directory)
    }
}

fn writable_directory(path: &Path) -> bool {
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // Test the signed-in process's effective permissions, including directory ACLs.
    unsafe {
        libc::faccessat(
            libc::AT_FDCWD,
            path.as_ptr(),
            libc::W_OK | libc::X_OK,
            libc::AT_EACCESS,
        ) == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_managed_files_are_excluded_from_all_personal_cleanup() {
        let policy = Policy {
            uid: 1000,
            roots: vec!["/tmp/account".into()],
        };
        for path in [
            ".cache/browser/blob",
            ".angi-agent/data/opencode/auth.json",
            "Downloads/project/target/release/app",
            "Documents/project/node_modules/bundle.js",
            ".local/share/app/database",
        ] {
            assert!(
                !policy.contains(&Path::new("/tmp/account").join(path)),
                "{path}"
            );
        }
        assert!(policy.contains(Path::new("/tmp/account/Downloads/game.AppImage")));
    }
}
