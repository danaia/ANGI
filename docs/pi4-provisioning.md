# Provision ANGI on Raspberry Pi 4 / 400

Build a card with your workstation’s SSH public key, boot the Pi over the direct USB-C link, and use the ANGI TUI to create the administrator and join Wi-Fi. The [README install prompt](../README.md#get-started-with-one-prompt) gives these steps to an agent that can run them for you.

## Build and write the card

From the repository root on the Linux workstation, set `SSH_PUBLIC_KEY` to an existing OpenSSH public key. If you need a new key pair, use a new filename and keep the private key on the workstation.

```sh
export SSH_PUBLIC_KEY="$HOME/.ssh/id_ed25519.pub"
make configure BOARD=rpi4
make dependencies BOARD=rpi4
make build BOARD=rpi4
make check-image BOARD=rpi4
(cd output-rpi4/images && sha256sum -c sdcard.img.sha256)
```

The build produces `output-rpi4/images/sdcard.img`. Keep the image for your own use because it grants initial root SSH access to the holder of the matching private key. Prepare a private install artifact and the matching SSH host pin with
`python3 scripts/provision-sd.py --images output-rpi4/images --output-dir PRIVATE_PARENT --public-key "$SSH_PUBLIC_KEY"`.
Verify the bundle with `python3 scripts/provision-sd.py --verify PRIVATE_DIR --board rpi4`.
Use the private artifact with `--personalized` when flashing and use its
`known_hosts` for strict SSH on the first boot.

Put the card in the workstation’s reader. Run `scripts/flash-sd.sh --list` and identify the whole card by its path, size, model, serial, and partitions. Writing will erase everything on the selected card. Substitute its verified path for `/dev/sdX` below and confirm the prompt. Wait for read-back verification and ejection before moving the card.

```sh
scripts/flash-sd.sh --board rpi4 --image PRIVATE_DIR/sdcard.img --device /dev/sdX --personalized
```

## Boot and provision over SSH

1. Insert the card in the powered-off Pi and connect a USB-C data cable. For direct Ethernet, connect the cable and use the Pi’s own power supply.
2. On a NetworkManager workstation, reuse the matching profile or create it once on the dedicated adapter. USB uses `agx-direct` for MAC `02:77:00:00:00:01`, workstation `10.77.0.1/24`, and Pi `10.77.0.2`. Direct Ethernet uses `agx-ethernet`, workstation `10.78.0.1/24`, and Pi `10.78.0.2`. Use no gateway and set `ipv4.never-default yes` and `ipv6.method disabled`. Check `ip route get PI_ADDRESS` for the expected device and source address.
3. Use the per-install trust file for the first SSH connection.

   ```sh
   pi=10.77.0.2
   pi_id=agx_rpi4
   trust=PRIVATE_DIR/known_hosts
   ssh -i "$HOME/.ssh/id_ed25519" -o IdentitiesOnly=yes -o "HostKeyAlias=$pi_id" \
     -o "UserKnownHostsFile=$trust" -o StrictHostKeyChecking=yes "root@$pi" \
     'cat /proc/device-tree/model; echo; agx status --json'
   ssh -i "$HOME/.ssh/id_ed25519" -o IdentitiesOnly=yes -o "HostKeyAlias=$pi_id" \
     -o "UserKnownHostsFile=$trust" -o StrictHostKeyChecking=yes -t "root@$pi" 'agx tui'
   ```

4. In **Profile** (`1`), set the hostname and create the administrator account. In **Network** (`4`), set the Wi-Fi country, scan, and choose **Connect & save**. Enter passwords in the terminal. Keep the direct SSH link available if you need to retry Wi-Fi.
5. Verify the Wi-Fi address with `agx wifi status --json` and DNS and HTTPS with `agx wifi check`. Confirm that the named administrator can sign in. The same per-device host alias can identify the Pi over USB, direct Ethernet, and Wi-Fi. Check `agx services --json` for the supervised services.

## Open the desktop

Use a free workstation loopback port for OpenSSH. This example uses `5943`. Reuse the per-device alias and workstation key from setup.

```sh
ssh -NT -i "$HOME/.ssh/id_ed25519" -o IdentitiesOnly=yes \
  -o "HostKeyAlias=$pi_id" -o "UserKnownHostsFile=$trust" -o StrictHostKeyChecking=yes \
  -o ExitOnForwardFailure=yes -L 127.0.0.1:5943:127.0.0.1:5900 "root@$pi"
```

Connect Remmina to `127.0.0.1:5943` with its built-in SSH tunnel disabled. Sign in with the ANGI administrator account. Use the desktop power menu or `F9` in the TUI to shut down safely.

The [hybrid ISO workflow](install-prompts/iso-headless.md) is also available for USB-C browser setup or provisioning from a previously downloaded generic image.
