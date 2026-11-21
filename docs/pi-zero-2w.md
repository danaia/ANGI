# ANGI on Raspberry Pi Zero 2 W

The experimental ARM64 target reuses the earlier Zero 2 W boot, USB and Wi-Fi configuration. It builds the shared ANGI desktop with Pebble Line light and dark themes on a headless software display. The desktop is available over VNC through SSH. This image provides the TUI, SSH and the desktop shell. HDMI desktop output, Bluetooth, the embedded browser and the on-device coding runtime are outside this build. The current image still needs a boot and desktop check on a physical Zero 2 W.

## Build and write

From the repository root on a Linux workstation, select or create your OpenSSH key with the provisioning helper.

```sh
SSH_PUBLIC_KEY=$(python3 scripts/ssh_provisioning.py)
export SSH_PUBLIC_KEY
make configure BOARD=rpizero2w
make dependencies BOARD=rpizero2w
make build BOARD=rpizero2w
make check-image BOARD=rpizero2w
(cd output-rpizero2w/images && sha256sum -c sdcard.img.sha256)
install_parent=$(mktemp -d "$HOME/angi-zero-install.XXXXXX")
python3 scripts/provision-sd.py --images output-rpizero2w/images \
  --public-key "$SSH_PUBLIC_KEY" --output-dir "$install_parent"
```

The provisioning script uses the workstation’s `dropbearkey` to prepare the private image and its `known_hosts` file. Keep the resulting directory private. Run `scripts/flash-sd.sh --list` to identify the whole card by path, size, model and contents. Confirm that erasing the chosen device is authorized, then use the card writer and wait for verification and ejection.

```sh
scripts/flash-sd.sh --board rpizero2w --image PRIVATE_DIR/sdcard.img \
  --device /dev/sdX --personalized
```

## Set up the board and open the desktop

1. Insert the card into the powered-off Zero 2 W. Connect a data cable to the **micro-USB port marked USB**. Use the separate power port if the workstation cannot reliably supply power.
2. Assign the workstation’s gadget adapter `10.79.0.1/24` with no gateway or default route. The adapter MAC is `02:79:00:00:00:01`. The Pi is at `10.79.0.2`. On NetworkManager, reuse `angi-zero-usb` if present.
3. Connect as root with the workstation’s matching private key, `UserKnownHostsFile` set to the private install directory’s `known_hosts` file, and strict host-key checking. Confirm the model with `cat /proc/device-tree/model`.
4. Open `agx tui` in an interactive SSH session. Create the administrator in **Profile**, then use **Network** to join a **2.4 GHz** network. Verify the connection with `agx wifi check` and confirm that the new account can sign in.
5. With the same SSH identity and trust file, forward a free workstation loopback port to the Pi’s loopback VNC listener. Point the VNC viewer at `127.0.0.1:5943` and check **Settings → Appearance** in light and dark.

```sh
ssh -NT -i WORKSTATION_KEY -o IdentitiesOnly=yes \
  -o StrictHostKeyChecking=yes -o UserKnownHostsFile=PRIVATE_DIR/known_hosts \
  -o ExitOnForwardFailure=yes -L 127.0.0.1:5943:127.0.0.1:5900 root@10.79.0.2
```

For Wi-Fi SSH, use the same trust file with `HostKeyAlias=10.79.0.2` and connect to the Pi’s Wi-Fi address.
