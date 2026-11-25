use super::*;
use std::os::unix::fs::symlink;

fn artifact(path: &str, bytes: &[u8]) -> Artifact {
    Artifact {
        path: path.to_owned(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
        size: bytes.len() as u64,
    }
}

#[test]
fn failed_verification_never_publishes_a_driver() {
    let root = Scratch::new(&std::env::temp_dir(), "test").unwrap();
    let firmware = artifact("firmware/vendor/fw.bin", b"valid");
    let module = artifact("modules/device.ko", b"module");
    let result = install_into(
        root.path(),
        root.path(),
        root.path(),
        &[&firmware, &module],
        "test-release",
        |entry, path| {
            fs::write(
                path,
                if entry.path == firmware.path {
                    b"valid".as_slice()
                } else {
                    b"broken".as_slice()
                },
            )
        },
    );
    assert!(result.is_err());
    assert!(!root.path().join("firmware/vendor/fw.bin").exists());
    assert!(
        !root
            .path()
            .join("modules/test-release/updates/angi/device.ko")
            .exists()
    );
}

#[test]
fn identical_retries_skip_download_and_existing_files_are_preserved() {
    let root = Scratch::new(&std::env::temp_dir(), "test").unwrap();
    let module = artifact("modules/device.ko", b"valid");
    assert_eq!(
        install_into(
            root.path(),
            root.path(),
            root.path(),
            &[&module],
            "test-release",
            |_, path| fs::write(path, b"valid")
        )
        .unwrap(),
        (true, true)
    );
    assert_eq!(
        install_into(
            root.path(),
            root.path(),
            root.path(),
            &[&module],
            "test-release",
            |_, _| panic!("unexpected download")
        )
        .unwrap(),
        (true, false)
    );
    let conflicting = artifact("modules/device.ko", b"other");
    assert!(
        install_into(
            root.path(),
            root.path(),
            root.path(),
            &[&conflicting],
            "test-release",
            |_, _| panic!("unexpected download")
        )
        .is_err()
    );
    assert_eq!(
        fs::read(
            root.path()
                .join("modules/test-release/updates/angi/device.ko")
        )
        .unwrap(),
        b"valid"
    );
}

#[test]
fn paths_cannot_escape_through_traversal_or_symlinks() {
    let root = Scratch::new(&std::env::temp_dir(), "test").unwrap();
    let outside = Scratch::new(&std::env::temp_dir(), "test").unwrap();
    symlink(outside.path(), root.path().join("firmware")).unwrap();
    for path in [
        "firmware/../escaped",
        "firmware/escaped",
        "modules/modules.alias",
        "/firmware/escaped",
    ] {
        let entry = artifact(path, b"invalid");
        assert!(
            install_into(
                root.path(),
                root.path(),
                root.path(),
                &[&entry],
                "test-release",
                |_, _| panic!("unexpected download")
            )
            .is_err()
        );
    }
    assert!(!outside.path().join("escaped").exists());
}

#[test]
fn graphics_bundles_accept_mesa_and_nvidia_runtime_files() {
    for name in [
        "graphics/libgbm.so.1",
        "graphics/libEGL.so.1",
        "graphics/libLLVM.so.22.1",
        "graphics/libSPIRV-Tools.so",
        "graphics/dri/iris_dri.so",
        "graphics/dri/radeonsi_dri.so",
        "graphics/gbm/dri_gbm.so",
        "graphics/gbm/nvidia-drm_gbm.so",
    ] {
        assert_eq!(route(name).unwrap().0, Area::GraphicsLibrary, "{name}");
    }
    assert_eq!(route("graphics/50_mesa.json").unwrap().0, Area::Glvnd);
    assert!(route("graphics/dri/subdir/invalid_dri.so").is_err());
    assert!(route("graphics/libc.so.6").is_err());
}
