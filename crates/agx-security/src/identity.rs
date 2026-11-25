//! This computer's public SSH key and host key, created on first use with whichever key tool
//! the image carries (OpenSSH's `ssh-keygen`, otherwise Dropbear's `dropbearkey`).
use crate::{Paths, sha256_fingerprint};
use std::{
    fs,
    io,
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    pub public_key: String,
    pub fingerprint: String,
}

fn run(program: &str, arguments: &[&str]) -> io::Result<String> {
    let output = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!("{program} failed")));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn key_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| line.starts_with("ssh-ed25519 "))
        .map(|line| line.split_whitespace().take(3).collect::<Vec<_>>().join(" "))
}

fn identity_of(line: String) -> Identity {
    Identity { fingerprint: sha256_fingerprint(&line).unwrap_or_default(), public_key: line }
}

/// The public half of Dropbear's host key: what a computer pins to recognize this one.
pub fn host_public_key(paths: &Paths) -> io::Result<Identity> {
    let key = paths.host_key.to_string_lossy();
    let text = run("dropbearkey", &["-y", "-f", &key])?;
    key_line(&text).map(identity_of).ok_or_else(|| io::Error::other("No host key"))
}

/// The key this computer offers other computers. Generated once, kept private to root.
pub fn ensure_client_key(paths: &Paths, hostname: &str) -> io::Result<Identity> {
    fs::create_dir_all(&paths.key_dir)?;
    fs::set_permissions(&paths.key_dir, fs::Permissions::from_mode(0o700))?;
    let openssh = paths.key_dir.join("id_ed25519");
    let dropbear = paths.key_dir.join("id_dropbear");
    if !openssh.exists() && !dropbear.exists() {
        let comment = format!("angi@{hostname}");
        let private = openssh.to_string_lossy().into_owned();
        // Try OpenSSH first so the same key works with the ssh client where one is installed.
        let made = run(
            "ssh-keygen",
            &["-q", "-t", "ed25519", "-N", "", "-C", &comment, "-f", &private],
        )
        .is_ok();
        if !made {
            let _ = fs::remove_file(&openssh);
            let target = dropbear.to_string_lossy().into_owned();
            run("dropbearkey", &["-t", "ed25519", "-f", &target])?;
        }
    }
    if openssh.exists() {
        let text = run("ssh-keygen", &["-y", "-f", &openssh.to_string_lossy()])?;
        return key_line(&text).map(identity_of).ok_or_else(|| io::Error::other("No client key"));
    }
    let text = run("dropbearkey", &["-y", "-f", &dropbear.to_string_lossy()])?;
    key_line(&text).map(identity_of).ok_or_else(|| io::Error::other("No client key"))
}
