//! Verify all downloads before publishing new files. Preserve existing drivers.
use super::catalog::Artifact;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU32, Ordering},
};

pub(super) fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

pub(super) fn component(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'+'))
}

pub(super) struct Scratch(PathBuf);

impl Scratch {
    pub(super) fn new(parent: &Path, role: &str) -> io::Result<Self> {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        loop {
            let serial = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                ".agx-drivers-{role}-{}-{serial}",
                std::process::id()
            ));
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.path());
    }
}

#[derive(Debug, PartialEq)]
enum Area {
    Firmware,
    GraphicsLibrary,
    Glvnd,
    EglPlatform,
    Modules,
}

fn route(path: &str) -> io::Result<(Area, &str)> {
    let invalid = || io::Error::other("driver file is outside the allowed installation paths");
    let (area, relative) = path.split_once('/').ok_or_else(invalid)?;
    if !relative.split('/').all(component) {
        return Err(invalid());
    }
    let area = match area {
        "firmware" => Area::Firmware,
        "graphics" if graphics_library(relative) => Area::GraphicsLibrary,
        "graphics" if matches!(relative, "10_nvidia.json" | "50_mesa.json") => Area::Glvnd,
        "graphics" if relative == "15_nvidia_gbm.json" => Area::EglPlatform,
        "modules"
            if [".ko", ".ko.xz", ".ko.zst", ".ko.gz"]
                .iter()
                .any(|suffix| relative.ends_with(suffix)) =>
        {
            Area::Modules
        }
        _ => return Err(invalid()),
    };
    Ok((area, relative))
}

fn graphics_library(name: &str) -> bool {
    matches!(name, "gbm/dri_gbm.so" | "gbm/nvidia-drm_gbm.so")
        || name
            .strip_prefix("dri/")
            .is_some_and(|driver| component(driver) && driver.ends_with("_dri.so"))
        || (!name.contains('/')
            && [
                "libEGL_nvidia.so",
                "libGLESv1_CM_nvidia.so",
                "libGLESv2_nvidia.so",
                "libnvidia-",
                "libcuda.so",
                "libgbm.so",
                "libEGL.so",
                "libEGL_mesa.so",
                "libGLdispatch.so",
                "libGLESv1_CM.so",
                "libGLESv2.so",
                "libgallium-",
                "libgallium.so",
                "libglapi.so",
                "libLLVM",
                "libSPIRV-Tools.so",
                "libdrm_amdgpu.so",
                "libdrm_intel.so",
                "libdrm_radeon.so",
                "libelf.so",
                "libexpat.so",
                "libffi.so",
                "libzstd.so",
                "libxml2.so",
                "libsensors.so",
                "libxcb-dri3.so",
                "libxcb-present.so",
                "libxcb-randr.so",
                "libxcb-sync.so",
                "libxcb-xfixes.so",
                "libxcb.so",
                "libxshmfence.so",
            ]
            .iter()
            .any(|prefix| name.starts_with(prefix))
            && (name.ends_with(".so") || name.contains(".so.")))
}

fn lock(path: &Path) -> io::Result<File> {
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(lock)
}

pub(super) fn install(
    artifacts: &[&Artifact],
    kernel: &str,
    fetch: impl Fn(&Artifact, &Path) -> io::Result<()>,
) -> io::Result<bool> {
    let _lock = lock(Path::new("/run/agx-driver-install.lock"))?;
    let lib = Path::new("/lib").canonicalize()?;
    for entry in fs::read_dir(&lib)? {
        let entry = entry?;
        if entry
            .file_name()
            .as_encoded_bytes()
            .starts_with(b".agx-drivers-install-")
            && entry.file_type()?.is_dir()
        {
            fs::remove_dir_all(entry.path())?;
        }
    }
    let user_lib = Path::new("/usr/lib").canonicalize()?;
    let share = Path::new("/usr/share").canonicalize()?;
    let (has_modules, changed) = install_into(&lib, &user_lib, &share, artifacts, kernel, fetch)?;
    if has_modules
        && !Command::new("depmod")
            .args(["-a", kernel])
            .status()?
            .success()
    {
        return Err(io::Error::other("driver dependency indexing failed"));
    }
    Ok(changed)
}

fn install_into(
    lib: &Path,
    user_lib: &Path,
    share: &Path,
    artifacts: &[&Artifact],
    kernel: &str,
    fetch: impl Fn(&Artifact, &Path) -> io::Result<()>,
) -> io::Result<(bool, bool)> {
    let stage = Scratch::new(lib, "install")?;
    let mut pending = Vec::new();
    let mut has_modules = false;
    for (number, artifact) in artifacts.iter().enumerate() {
        let (area, relative) = route(&artifact.path)?;
        let (root, directory, relative) = match area {
            Area::Firmware => (lib, "firmware".to_owned(), relative),
            Area::GraphicsLibrary => (user_lib, String::new(), relative),
            Area::Glvnd => (share, "glvnd/egl_vendor.d".to_owned(), relative),
            Area::EglPlatform => (share, "egl/egl_external_platform.d".to_owned(), relative),
            Area::Modules => {
                has_modules = true;
                (lib, format!("modules/{kernel}/updates/angi"), relative)
            }
        };
        let relative = if directory.is_empty() {
            relative.to_owned()
        } else {
            format!("{directory}/{relative}")
        };
        let path = destination(root, &relative)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() => {
                verify_file(&path, artifact)?;
                continue;
            }
            Ok(_) => return Err(io::Error::other("driver destination is not a regular file")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let temp = stage.path().join(number.to_string());
        fetch(artifact, &temp)?;
        verify_file(&temp, artifact)?;
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o644))?;
        File::open(&temp)?.sync_all()?;
        pending.push((area, temp, path));
    }

    // Publish dependencies first, then loader configuration, then kernel modules.
    // hard_link never overwrites an existing path.
    pending.sort_by_key(|(area, _, _)| match area {
        Area::Firmware | Area::GraphicsLibrary => 0,
        Area::Glvnd | Area::EglPlatform => 1,
        Area::Modules => 2,
    });
    let changed = !pending.is_empty();
    let mut published = Vec::new();
    let publish = (|| -> io::Result<()> {
        for (_, temp, path) in &pending {
            fs::hard_link(temp, path)?;
            published.push(path);
            if let Some(parent) = path.parent() {
                File::open(parent)?.sync_all()?;
            }
        }
        Ok(())
    })();
    if let Err(error) = publish {
        for path in published.into_iter().rev() {
            let _ = fs::remove_file(path);
        }
        return Err(error);
    }
    Ok((has_modules, changed))
}

fn destination(root: &Path, path: &str) -> io::Result<PathBuf> {
    let Some((directories, filename)) = path.rsplit_once('/') else {
        return Ok(root.join(path));
    };
    let mut parent = root.to_owned();
    for name in directories.split('/') {
        parent.push(name);
        match fs::create_dir(&parent) {
            Ok(()) => {
                if let Some(previous) = parent.parent() {
                    File::open(previous)?.sync_all()?;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        if !fs::symlink_metadata(&parent)?.is_dir() {
            return Err(io::Error::other(
                "driver destination crosses a symbolic link",
            ));
        }
    }
    Ok(parent.join(filename))
}

fn verify_file(path: &Path, artifact: &Artifact) -> io::Result<()> {
    let mut file = File::open(path)?;
    if file.metadata()?.len() != artifact.size {
        return Err(io::Error::other("driver file size mismatch"));
    }
    let mut sha = Sha256::new();
    io::copy(&mut file, &mut sha)?;
    if format!("{:x}", sha.finalize()) != artifact.sha256 {
        return Err(io::Error::other("driver file checksum mismatch"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
