use super::{acceleration, inventory, online, row};
use std::{fs, os::unix::fs::symlink, path::PathBuf};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("angi-drivers-{}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn link(&self, name: &str, target: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let target = if target.starts_with('/') {
            PathBuf::from(target)
        } else {
            self.0.join(target)
        };
        symlink(target, path).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn inventory_merges_bound_devices_and_lists_hardware_with_no_endpoint() {
    let f = Fixture::new();
    f.write("proc/cpuinfo", "model name : AMD Ryzen 7 5700U\n");
    f.write("proc/sys/kernel/osrelease", "6.12.47");
    f.write("sys/devices/display/class", "0x030000\n");
    f.write("sys/devices/display/vendor", "0x1002\n");
    f.write("sys/devices/display/device", "0x164c\n");
    f.link("sys/devices/display/subsystem", "/bus/pci");
    f.link("sys/devices/display/driver", "/bus/pci/drivers/amdgpu");
    f.link("sys/bus/pci/devices/0000:04:00.0", "sys/devices/display");
    f.link("sys/class/drm/card0/device", "sys/devices/display");

    f.write(
        "modules/6.12.47/modules.builtin.modinfo",
        "iwlwifi.alias=pci:v00008086d00002723sv*sd*bc*sc*i*\0",
    );
    f.write(
        "sys/devices/wifi/modalias",
        "pci:v00008086d00002723sv00008086sd00000084bc02sc80i00",
    );
    f.write("sys/devices/wifi/class", "0x028000");
    f.write("sys/devices/wifi/vendor", "0x8086");
    f.write("sys/devices/wifi/device", "0x2723");
    f.link("sys/devices/wifi/subsystem", "/bus/pci");
    f.link("sys/bus/pci/devices/0000:02:00.0", "sys/devices/wifi");

    f.write("sys/devices/usb/hub/idVendor", "8087");
    f.write("sys/devices/usb/hub/idProduct", "0029");
    f.link("sys/devices/usb/hub/subsystem", "/bus/usb");
    f.write("sys/devices/usb/hub/if/bInterfaceClass", "e0");
    f.write("sys/devices/usb/hub/if/bInterfaceSubClass", "01");
    f.write("sys/devices/usb/hub/if/bInterfaceProtocol", "01");
    f.link("sys/devices/usb/hub/if/subsystem", "/bus/usb");
    f.link("sys/bus/usb/devices/1-1:1.0", "sys/devices/usb/hub/if");

    f.write(
        "sys/devices/v3d/of_node/compatible",
        "brcm,2712-v3d\0brcm,2711-v3d\0",
    );
    f.link("sys/devices/v3d/subsystem", "/bus/platform");
    f.link("sys/bus/platform/devices/107c800000.v3d", "sys/devices/v3d");

    let snapshot = inventory::scan(&f.0.join("sys"), &f.0.join("proc"), &f.0.join("modules"));
    assert!(snapshot.model.starts_with("AMD Ryzen"));
    assert_eq!(snapshot.kernel, "6.12.47");
    let graphics = &snapshot.groups[&inventory::Kind::Graphics];
    assert_eq!(graphics.len(), 2);
    assert_eq!(graphics[0].id.unwrap().display(), "PCI 1002:164c");
    assert_eq!(graphics[0].endpoints, ["card0"]);
    assert_eq!(row(&graphics[0], None).status, "In use");
    assert_eq!(graphics[1].compatible, "brcm,2712-v3d");
    let wifi = &snapshot.groups[&inventory::Kind::Wifi][0];
    assert_eq!(wifi.id.unwrap().display(), "PCI 8086:2723");
    assert_eq!(row(wifi, None).status, "Installed, inactive");
    assert_eq!(wifi.candidates, ["iwlwifi"]);
    assert_eq!(row(&graphics[1], None).status, "Driver not found");
    let bluetooth = &snapshot.groups[&inventory::Kind::Bluetooth][0];
    assert_eq!(bluetooth.id.unwrap().display(), "USB 8087:0029");

    f.write(
        "sys/firmware/devicetree/base/model",
        "Raspberry Pi 5 Model B\0",
    );
    assert_eq!(
        inventory::scan(&f.0.join("sys"), &f.0.join("proc"), &f.0.join("modules")).model,
        "Raspberry Pi 5 Model B"
    );
}

#[test]
fn online_lookup_uses_only_the_requested_devices_driver_table() {
    let id = inventory::BusId {
        bus: "pci",
        vendor: 0x1002,
        product: 0x164c,
    };
    let html = "<h2 class='top'>Device 'AMD Lucienne'</h2>PCI 1002:164c\n
        <a name='kernel_drivers'></a><table>
        <tr><td>5.10</td><td>arch/x86/kernel/early-quirks.c</td><td>CONFIG_PCI</td></tr>
        <tr><td>5.10</td><td>sound/hda/controller.c</td><td>CONFIG_SND_HDA_CORE</td></tr>
        <tr><td>5.10</td><td>drivers/gpu/drm/amd/amdgpu/driver.c</td><td>CONFIG_DRM_AMDGPU CONFIG_DRM_AMDGPU</td></tr>
        </table><td>CONFIG_UNRELATED</td>";
    assert_eq!(
        online::parse_device(html, inventory::Kind::Graphics, id),
        online::Result::Found {
            name: "AMD Lucienne".into(),
            configs: vec!["CONFIG_DRM_AMDGPU".into()],
        }
    );
    assert_eq!(
        online::parse_device(
            &html.replace("1002:164c", "8086:4e55"),
            inventory::Kind::Graphics,
            id
        ),
        online::Result::Unavailable
    );
}

#[test]
fn current_gl_context_distinguishes_gpu_and_cpu_rasterizers() {
    assert_eq!(
        acceleration("AMD Radeon Graphics (radeonsi, renoir)"),
        ("Hardware accelerated", 1)
    );
    assert_eq!(
        acceleration("Mesa Intel(R) UHD Graphics (JSL)"),
        ("Hardware accelerated", 1)
    );
    assert_eq!(acceleration("V3D 7.1.7"), ("Hardware accelerated", 1));
    assert_eq!(
        acceleration("llvmpipe (LLVM 22, 256 bits)"),
        ("Software rendering", 2)
    );
    assert_eq!(acceleration("unidentified context"), ("OpenGL active", 0));
}
