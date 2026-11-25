use super::*;

#[test]
fn a_kernel_usb_class_alias_matches_across_vendors() {
    let selector: Selector =
        parse(br#"{"bus":"modalias","pattern":"usb:v*p*d*dc*dsc*dp*ic01isc01ip*in*"}"#).unwrap();
    let hardware = |alias| Hardware {
        modaliases: [CString::new(alias).unwrap()].into(),
        ..Hardware::default()
    };
    assert!(selector.matches(&hardware(
        "usb:v46F4p0002d0001dc00dsc00dp00ic01isc01ip00in00"
    )));
    assert!(selector.matches(&hardware(
        "usb:v1234p5678d0002dc00dsc00dp00ic01isc01ip20in00"
    )));
    assert!(!selector.matches(&hardware(
        "usb:v1234p5678d0002dc00dsc00dp00icE0isc01ip01in00"
    )));
}

#[test]
fn existing_catalog_configurations_keep_directory_paths() {
    let mut config = serde_json::json!({"url": "https://example.test", "kernel_build": "build"});
    let legacy: Config = serde_json::from_value(config.clone()).unwrap();
    assert_eq!(legacy.asset_layout.path("digest"), "files/digest");
    config["asset_layout"] = "release".into();
    let release: Config = serde_json::from_value(config.clone()).unwrap();
    assert_eq!(release.asset_layout.path("digest"), "driver-digest");
    config["asset_layout"] = "unsupported".into();
    assert!(serde_json::from_value::<Config>(config).is_err());
}

#[test]
fn selects_mixed_vendor_hardware_and_only_the_exact_kernel_build() {
    let hardware = Hardware {
        ids: [
            BusId {
                bus: "pci",
                vendor: 0x10de,
                product: 0x2690,
            },
            BusId {
                bus: "pci",
                vendor: 0x1002,
                product: 0x164c,
            },
            BusId {
                bus: "pci",
                vendor: 0x8086,
                product: 0x2723,
            },
        ]
        .into(),
        ..Hardware::default()
    };
    let bundle = |arch, build, vendor, product, name| {
        serde_json::json!({
            "arch": arch, "kernel": "6.12.47", "kernel_build": build,
            "devices": [{"bus": "pci", "vendor": vendor, "product": product}],
            "files": [{"path": name, "size": 10, "sha256": "0".repeat(64)}]
        })
    };
    let json = serde_json::json!({
        "version": 1,
        "bundles": [
            bundle(std::env::consts::ARCH, "current", 0x10de, 0x2690, "modules/nvidia.ko"),
            bundle(std::env::consts::ARCH, "current", 0x1002, 0x164c, "modules/amdgpu.ko"),
            bundle(std::env::consts::ARCH, "current", 0x8086, 0x2723, "modules/iwlwifi.ko"),
            bundle(std::env::consts::ARCH, "different", 0x10de, 0x2690, "modules/wrong-kernel.ko"),
            bundle(std::env::consts::ARCH, "current", 0x8086, 0x4e55, "modules/i915.ko"),
            bundle("different", "current", 0x10de, 0x2690, "modules/wrong-architecture.ko")
        ]
    });
    let index: Index = serde_json::from_value(json).unwrap();
    let files = select(&index, &hardware, "6.12.47", "current").unwrap();
    let paths: Vec<_> = files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "modules/amdgpu.ko",
            "modules/iwlwifi.ko",
            "modules/nvidia.ko"
        ]
    );
}

#[test]
fn verifies_catalog_with_release_key_and_rejects_modified_bytes() {
    let work = files::Scratch::new(&std::env::temp_dir(), "test").unwrap();
    let key = work.path().join("key");
    assert!(
        Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(&key)
            .status()
            .unwrap()
            .success()
    );
    let signers = work.path().join("signers");
    let public = fs::read_to_string(key.with_extension("pub")).unwrap();
    fs::write(&signers, format!("angi-drivers {public}")).unwrap();
    let index = work.path().join("index.json");
    let signature = work.path().join("index.json.sig");
    fs::write(&index, br#"{"version":1,"bundles":[]}"#).unwrap();
    assert!(
        Command::new("ssh-keygen")
            .args(["-Y", "sign", "-n", "angi-driver-catalog", "-f"])
            .arg(&key)
            .arg(&index)
            .output()
            .unwrap()
            .status
            .success()
    );
    verify(&index, &signature, &signers).unwrap();
    fs::write(&index, br#"{"version":2,"bundles":[]}"#).unwrap();
    assert!(verify(&index, &signature, &signers).is_err());
}

#[test]
fn pci_subsystem_distinguishes_board_variants_with_the_same_chip() {
    use std::os::unix::fs::symlink;

    let sys = files::Scratch::new(&std::env::temp_dir(), "pci-variants").unwrap();
    let bus = sys.path().join("bus/pci");
    let device = bus.join("devices/0000:03:00.0");
    fs::create_dir_all(&device).unwrap();
    symlink(&bus, device.join("subsystem")).unwrap();
    for (name, contents) in [
        ("vendor", "0x8086"),
        ("device", "0x2723"),
        ("subsystem_vendor", "0x1234"),
        ("subsystem_device", "0x5678"),
    ] {
        fs::write(device.join(name), contents).unwrap();
    }
    let hardware = inventory::catalog_hardware(sys.path());
    let selector = |subsystem| {
        parse::<Selector>(
            &serde_json::to_vec(&serde_json::json!({
                "bus": "pci", "vendor": 0x8086, "product": 0x2723,
                "subsystem": subsystem
            }))
            .unwrap(),
        )
        .unwrap()
    };
    assert!(selector(serde_json::Value::Null).matches(&hardware));
    assert!(selector(serde_json::json!({"vendor": 0x1234, "product": 0x5678})).matches(&hardware));
    assert!(!selector(serde_json::json!({"vendor": 0x1234, "product": 0x5679})).matches(&hardware));
    fs::remove_file(device.join("subsystem_device")).unwrap();
    let hardware = inventory::catalog_hardware(sys.path());
    assert!(!selector(serde_json::json!({"vendor": 0x1234, "product": 0x5678})).matches(&hardware));
}
