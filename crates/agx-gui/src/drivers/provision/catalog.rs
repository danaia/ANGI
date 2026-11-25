//! Signed, exact-build catalog shared by a release channel.
use super::{Inventory, files};
use crate::drivers::inventory::{self, BusId, Hardware};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fs, io,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const MAX_INDEX: u64 = 512 * 1024;
const MAX_SIGNATURE: u64 = 16 * 1024;
const MAX_FILES: usize = 256;
const MAX_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    url: String,
    kernel_build: String,
    #[serde(default)]
    asset_layout: AssetLayout,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "lowercase")]
enum AssetLayout {
    #[default]
    Directory,
    Release,
}

impl AssetLayout {
    fn path(&self, digest: &str) -> String {
        match self {
            Self::Directory => format!("files/{digest}"),
            Self::Release => format!("driver-{digest}"),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    version: u32,
    bundles: Vec<Bundle>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    arch: String,
    kernel: String,
    kernel_build: String,
    devices: Vec<Selector>,
    files: Vec<Artifact>,
}

#[derive(Deserialize)]
#[serde(tag = "bus", rename_all = "lowercase", deny_unknown_fields)]
enum Selector {
    Pci {
        vendor: u16,
        product: u16,
        subsystem: Option<PciSubsystem>,
    },
    Usb {
        vendor: u16,
        product: u16,
    },
    Sdio {
        vendor: u16,
        product: u16,
    },
    DeviceTree {
        compatible: String,
    },
    Modalias {
        pattern: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PciSubsystem {
    vendor: u16,
    product: u16,
}

impl Selector {
    fn matches(&self, hardware: &Hardware) -> bool {
        let (bus, vendor, product) = match self {
            Self::Pci {
                vendor,
                product,
                subsystem,
            } => {
                if let Some(subsystem) = subsystem {
                    let id = BusId {
                        bus: "pci",
                        vendor: *vendor,
                        product: *product,
                    };
                    return hardware.pci_subsystems.contains(&(
                        id,
                        subsystem.vendor,
                        subsystem.product,
                    ));
                }
                ("pci", *vendor, *product)
            }
            Self::Usb { vendor, product } => ("usb", *vendor, *product),
            Self::Sdio { vendor, product } => ("sdio", *vendor, *product),
            Self::DeviceTree { compatible } => {
                return hardware.compatibles.contains(compatible);
            }
            Self::Modalias { pattern } => {
                if !pattern.starts_with("pci:") && !pattern.starts_with("usb:") {
                    return false;
                }
                return CString::new(pattern.as_str()).is_ok_and(|pattern| {
                    hardware
                        .modaliases
                        .iter()
                        .any(|alias| inventory::matches(&pattern, alias))
                });
            }
        };
        hardware.ids.contains(&BusId {
            bus,
            vendor,
            product,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Artifact {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

pub(super) enum Status {
    Unconfigured,
    NoMatch,
    Installed,
}

#[derive(Debug)]
pub(super) struct FeedAccessRequired;

impl std::fmt::Display for FeedAccessRequired {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the driver feed refused anonymous access")
    }
}

impl std::error::Error for FeedAccessRequired {}

pub(super) fn prepare(inventory: &Inventory) -> io::Result<Status> {
    let config = match fs::read("/etc/agx/driver-catalog.json") {
        Ok(data) => parse::<Config>(&data)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Status::Unconfigured),
        Err(error) => return Err(error),
    };
    if fs::read_to_string("/proc/cmdline")?
        .split_whitespace()
        .any(|option| option == "agx.driver_recovery=1")
    {
        return Ok(Status::Unconfigured);
    }
    let base = config.url.trim_end_matches('/');
    if !base.starts_with("https://")
        || base[8..].is_empty()
        || base
            .bytes()
            .any(|byte| matches!(byte, b'?' | b'#' | b'%' | b'@') || byte.is_ascii_whitespace())
        || !files::valid_digest(&config.kernel_build)
        || !files::component(&inventory.kernel)
    {
        return Err(io::Error::other("invalid driver catalog configuration"));
    }

    let notes = fs::read("/sys/kernel/notes")?;
    if notes.is_empty() || format!("{:x}", Sha256::digest(notes)) != config.kernel_build {
        return Err(io::Error::other(
            "running kernel differs from the driver catalog configuration",
        ));
    }

    wait_for_clock();
    let deadline = Instant::now() + Duration::from_secs(240);
    let work = files::Scratch::new(Path::new("/run"), "catalog")?;
    let index = work.path().join("index.json");
    let signature = work.path().join("index.json.sig");
    download(base, "index.json", &index, MAX_INDEX, deadline)?;
    download(base, "index.json.sig", &signature, MAX_SIGNATURE, deadline)?;
    verify(&index, &signature, Path::new("/etc/agx/driver-signers"))?;
    let index = parse::<Index>(&fs::read(index)?)?;
    if index.version != 1 {
        return Err(io::Error::other("unsupported driver catalog version"));
    }
    let hardware = inventory::catalog_hardware(Path::new("/sys"));
    let artifacts = select(&index, &hardware, &inventory.kernel, &config.kernel_build)?;
    if artifacts.is_empty() {
        return Ok(Status::NoMatch);
    }
    let changed = files::install(&artifacts, &inventory.kernel, |artifact, path| {
        download(
            base,
            &config.asset_layout.path(&artifact.sha256),
            path,
            artifact.size,
            deadline,
        )
    })?;
    if changed {
        fs::write("/run/agx-driver-restart-required", b"")?;
    }
    Ok(Status::Installed)
}

fn wait_for_clock() {
    const JAN_2025: u64 = 1_735_689_600;
    let deadline = Instant::now() + Duration::from_secs(12);
    while std::time::SystemTime::UNIX_EPOCH
        .elapsed()
        .is_ok_and(|age| age.as_secs() < JAN_2025)
        && Instant::now() < deadline
    {
        if !agx_net::request(&agx_net::Request::Status).is_ok_and(|network| {
            network.ok && network.status.phase == "Connected" && !network.status.time_synced
        }) {
            break;
        }
        std::thread::sleep(Duration::from_millis(350));
    }
}

fn select<'a>(
    index: &'a Index,
    hardware: &Hardware,
    kernel: &str,
    build: &str,
) -> io::Result<Vec<&'a Artifact>> {
    let mut paths = std::collections::BTreeMap::new();
    for bundle in &index.bundles {
        if bundle.arch != std::env::consts::ARCH
            || bundle.kernel != kernel
            || bundle.kernel_build != build
        {
            continue;
        }
        if !bundle
            .devices
            .iter()
            .any(|selector| selector.matches(hardware))
        {
            continue;
        }
        for artifact in &bundle.files {
            if let Some(previous) = paths.insert(artifact.path.as_str(), artifact) {
                if previous.sha256 != artifact.sha256 || previous.size != artifact.size {
                    return Err(io::Error::other("conflicting driver files in catalog"));
                }
            }
        }
    }
    let mut total = 0_u64;
    for artifact in paths.values() {
        if !files::valid_digest(&artifact.sha256) || artifact.size == 0 {
            return Err(io::Error::other("invalid driver file metadata"));
        }
        total = total.saturating_add(artifact.size);
    }
    if paths.len() > MAX_FILES || total > MAX_BYTES {
        return Err(io::Error::other(
            "driver bundles exceed installation limits",
        ));
    }
    Ok(paths.into_values().collect())
}

fn verify(index: &Path, signature: &Path, signers: &Path) -> io::Result<()> {
    let result = Command::new("ssh-keygen")
        .args(["-Y", "verify", "-f"])
        .arg(signers)
        .args(["-I", "angi-drivers", "-n", "angi-driver-catalog", "-s"])
        .arg(signature)
        .stdin(fs::File::open(index)?)
        .output()?;
    if !result.status.success() {
        return Err(io::Error::other("invalid driver catalog signature"));
    }
    Ok(())
}

fn download(
    base: &str,
    remote: &str,
    destination: &Path,
    limit: u64,
    deadline: Instant,
) -> io::Result<()> {
    let remaining = deadline.saturating_duration_since(Instant::now()).as_secs();
    if remaining == 0 {
        return Err(io::Error::other("driver download timed out"));
    }
    let response = Command::new("curl")
        .args([
            "-q",
            "--fail",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--location",
            "--proto-redir",
            "=https",
            "--max-redirs",
            "5",
            "--connect-timeout",
            "10",
            "--write-out",
            "%{response_code}",
        ])
        .args([
            "--max-time",
            &remaining.to_string(),
            "--max-filesize",
            &limit.to_string(),
        ])
        .arg("--output")
        .arg(destination)
        .arg("--")
        .arg(format!("{base}/{remote}"))
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()?;
    if matches!(response.stdout.as_slice(), b"401" | b"403") {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            FeedAccessRequired,
        ));
    }
    if !response.status.success() || fs::metadata(destination)?.len() > limit {
        return Err(io::Error::other("driver download failed"));
    }
    Ok(())
}

fn parse<T: serde::de::DeserializeOwned>(data: &[u8]) -> io::Result<T> {
    serde_json::from_slice(data).map_err(io::Error::other)
}

#[cfg(test)]
mod tests;
