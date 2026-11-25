//! Class endpoints establish working bindings. Bus enumeration also finds devices
//! whose driver is missing or never registered an endpoint.
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    ffi::{CStr, CString},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) enum Kind {
    Graphics,
    Sound,
    Wifi,
    Bluetooth,
}

impl Kind {
    pub(super) const ALL: [Self; 4] = [Self::Graphics, Self::Sound, Self::Wifi, Self::Bluetooth];
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct BusId {
    pub bus: &'static str,
    pub vendor: u16,
    pub product: u16,
}

impl BusId {
    pub(super) fn display(self) -> String {
        format!(
            "{} {:04x}:{:04x}",
            self.bus.to_ascii_uppercase(),
            self.vendor,
            self.product
        )
    }
}

#[derive(Debug)]
pub(super) struct Device {
    pub name: String,
    pub id: Option<BusId>,
    pub compatible: String,
    pub bound: String,
    pub candidates: Vec<String>,
    pub endpoints: Vec<String>,
}

#[derive(Debug)]
pub(super) struct Inventory {
    pub model: String,
    pub kernel: String,
    pub groups: BTreeMap<Kind, Vec<Device>>,
}

#[derive(Default)]
pub(super) struct Hardware {
    pub ids: HashSet<BusId>,
    pub pci_subsystems: HashSet<(BusId, u16, u16)>,
    pub compatibles: HashSet<String>,
    pub modaliases: HashSet<CString>,
}

pub(super) fn catalog_hardware(sys: &Path) -> Hardware {
    let mut hardware = Hardware::default();
    for bus in ["pci", "usb", "sdio", "platform", "serial", "serdev"] {
        for entry in entries(sys.join("bus").join(bus).join("devices")) {
            let Ok(path) = entry.canonicalize() else {
                continue;
            };
            if let Some(id) = bus_id(&path) {
                hardware.ids.insert(id);
                if id.bus == "pci"
                    && let (Some(vendor), Some(product)) = (
                        hex(path.join("subsystem_vendor"))
                            .and_then(|value| u16::try_from(value).ok()),
                        hex(path.join("subsystem_device"))
                            .and_then(|value| u16::try_from(value).ok()),
                    )
                {
                    hardware.pci_subsystems.insert((id, vendor, product));
                }
            }
            hardware.compatibles.extend(dt_compatible(&path));
            let alias = read(path.join("modalias"));
            if !alias.is_empty()
                && let Ok(alias) = CString::new(alias)
            {
                hardware.modaliases.insert(alias);
            }
        }
    }
    hardware
}

fn read(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn hex(path: impl AsRef<Path>) -> Option<u32> {
    u32::from_str_radix(read(path).trim_start_matches("0x"), 16).ok()
}

fn entries(path: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    paths
}

fn leaf(path: &Path) -> &str {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
}

fn bus(path: &Path) -> String {
    fs::read_link(path.join("subsystem"))
        .ok()
        .map(|path| leaf(&path).to_owned())
        .unwrap_or_default()
}

fn bus_id(path: &Path) -> Option<BusId> {
    for path in path.ancestors() {
        let (bus, vendor, device) = match bus(path).as_str() {
            "pci" => ("pci", "vendor", "device"),
            "sdio" => ("sdio", "vendor", "device"),
            "usb" if path.join("idVendor").is_file() => ("usb", "idVendor", "idProduct"),
            "usb" => continue, // USB function, whose IDs are on its physical device.
            _ => return None,  // A platform device is not its parent's PCI controller.
        };
        return Some(BusId {
            bus,
            vendor: u16::try_from(hex(path.join(vendor))?).ok()?,
            product: u16::try_from(hex(path.join(device))?).ok()?,
        });
    }
    None
}

fn dt_compatible(path: &Path) -> Vec<String> {
    read(path.join("of_node/compatible"))
        .split('\0')
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn driver(path: &Path) -> String {
    fs::read_link(path.join("driver"))
        .ok()
        .map(|path| leaf(&path).to_owned())
        .unwrap_or_default()
}

#[derive(Default)]
struct ModuleAliases(Vec<(CString, String)>);

impl ModuleAliases {
    fn load(dir: &Path) -> Self {
        let module_aliases = read(dir.join("modules.alias"));
        let builtins = read(dir.join("modules.builtin.modinfo"));
        let aliases = module_aliases.lines().filter_map(|line| {
            let mut words = line.split_whitespace();
            (words.next()? == "alias").then_some(())?;
            Some((words.next()?, words.next()?))
        });
        let builtin_aliases = builtins.split('\0').filter_map(|record| {
            let (module, pattern) = record.split_once(".alias=")?;
            Some((pattern, module))
        });
        Self(
            aliases
                .chain(builtin_aliases)
                .filter_map(|(pattern, module)| Some((CString::new(pattern).ok()?, module.into())))
                .collect(),
        )
    }

    fn matching(&self, path: &Path) -> Vec<String> {
        let Ok(alias) = CString::new(read(path.join("modalias"))) else {
            return Vec::new();
        };
        if alias.as_bytes().is_empty() {
            return Vec::new();
        }
        self.0
            .iter()
            .filter(|(pattern, _)| matches(pattern, &alias))
            .map(|(_, module)| module.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

pub(super) fn matches(pattern: &CStr, alias: &CStr) -> bool {
    // Match the kernel's modalias glob syntax without running a module loader.
    unsafe { libc::fnmatch(pattern.as_ptr(), alias.as_ptr(), libc::FNM_NOESCAPE) == 0 }
}

fn platform_kind(compatible: &str) -> Option<Kind> {
    let (vendor, part) = compatible.split_once(',')?;
    match (vendor, part) {
        ("brcm", part)
            if ["-v3d", "-vc4", "-vc5", "-vc6"]
                .iter()
                .any(|suffix| part.ends_with(suffix)) =>
        {
            Some(Kind::Graphics)
        }
        ("brcm", part) if part.ends_with("-wifi") => Some(Kind::Wifi),
        ("brcm", part) if part.ends_with("-bt") => Some(Kind::Bluetooth),
        ("brcm" | "raspberrypi", part) if part.ends_with("-audio") => Some(Kind::Sound),
        _ => None,
    }
}

fn bus_kind(path: &Path) -> Option<Kind> {
    match bus(path).as_str() {
        "pci" => match hex(path.join("class"))? >> 8 {
            0x0300..=0x03ff => Some(Kind::Graphics),
            0x0401 | 0x0403 | 0x0480 => Some(Kind::Sound),
            0x0280 => Some(Kind::Wifi),
            _ => None,
        },
        "usb" => match (
            hex(path.join("bInterfaceClass"))?,
            hex(path.join("bInterfaceSubClass"))?,
            hex(path.join("bInterfaceProtocol"))?,
        ) {
            (1, 1, _) => Some(Kind::Sound),
            (0xe0, 1, 1) => Some(Kind::Bluetooth),
            _ => None,
        },
        "sdio"
            if hex(path.join("class")) == Some(7)
                || driver(path) == "brcmfmac"
                || bus_id(path).is_some_and(|id| {
                    id.vendor == 0x02d0 && matches!(id.product, 0xa9a6 | 0xa9af | 0xa9bf | 0xbd3d)
                })
                || dt_compatible(path)
                    .iter()
                    .any(|c| platform_kind(c) == Some(Kind::Wifi)) =>
        {
            Some(Kind::Wifi)
        }
        _ => dt_compatible(path).iter().find_map(|c| platform_kind(c)),
    }
}

fn numbered(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

fn insert(
    devices: &mut BTreeMap<(Kind, PathBuf), Device>,
    kind: Kind,
    path: PathBuf,
    endpoint: Option<String>,
    modules: &ModuleAliases,
) {
    let id = bus_id(&path);
    let address = if id.is_some_and(|id| id.bus == "usb") && !path.join("idVendor").is_file() {
        path.parent().unwrap_or(&path)
    } else {
        &path
    };
    let bound = driver(&path);
    let device = devices
        .entry((kind, address.to_owned()))
        .or_insert_with(|| {
            let compatible = dt_compatible(&path).into_iter().next().unwrap_or_default();
            let name = if compatible.is_empty() {
                leaf(address).into()
            } else {
                compatible.clone()
            };
            let candidates = if bound.is_empty() {
                modules.matching(&path)
            } else {
                Vec::new()
            };
            Device {
                name,
                id,
                compatible,
                bound: bound.clone(),
                candidates,
                endpoints: Vec::new(),
            }
        });
    if device.bound.is_empty() && !bound.is_empty() {
        device.bound = bound;
        device.candidates.clear();
    }
    if let Some(endpoint) = endpoint {
        device.endpoints.push(endpoint);
    }
}

fn model(sys: &Path, proc: &Path) -> String {
    let board = read(sys.join("firmware/devicetree/base/model"))
        .trim_end_matches('\0')
        .to_owned();
    if !board.is_empty() {
        return board;
    }
    let cpu = read(proc.join("cpuinfo"));
    if let Some(model) = cpu.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim() == "model name")
            .then_some(value.trim())
            .filter(|value| !value.is_empty())
    }) {
        return model.into();
    }
    let dmi = read(sys.join("class/dmi/id/product_name"));
    if !dmi.is_empty() {
        dmi
    } else {
        std::env::consts::ARCH.into()
    }
}

pub(super) fn scan(sys: &Path, proc: &Path, module_root: &Path) -> Inventory {
    let kernel = read(proc.join("sys/kernel/osrelease"));
    let modules = ModuleAliases::load(&module_root.join(&kernel));
    let classes = [
        (Kind::Graphics, "drm", "card"),
        (Kind::Sound, "sound", "card"),
        (Kind::Wifi, "ieee80211", "phy"),
        (Kind::Bluetooth, "bluetooth", "hci"),
    ];
    let mut devices = BTreeMap::new();
    for (kind, class, prefix) in classes {
        for endpoint in entries(sys.join("class").join(class)) {
            if !numbered(leaf(&endpoint), prefix) {
                continue;
            }
            if let Ok(path) = endpoint.join("device").canonicalize() {
                let label = read(endpoint.join("id"));
                let label = if label.is_empty() {
                    leaf(&endpoint).to_owned()
                } else {
                    label
                };
                insert(&mut devices, kind, path, Some(label), &modules);
            }
        }
    }
    for bus in ["pci", "usb", "sdio", "platform", "serial", "serdev"] {
        for entry in entries(sys.join("bus").join(bus).join("devices")) {
            let Ok(path) = entry.canonicalize() else {
                continue;
            };
            if let Some(kind) = bus_kind(&path) {
                insert(&mut devices, kind, path, None, &modules);
            }
        }
    }
    let mut groups: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for ((kind, _), device) in devices {
        groups.entry(kind).or_default().push(device);
    }
    Inventory {
        model: model(sys, proc),
        kernel,
        groups,
    }
}
