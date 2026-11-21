# Install ANGI over direct Ethernet

This experimental route is for compatible Pi 4, Pi 5 and 64-bit UEFI network
boot firmware. It uses a temporary RAM-only installer. Use a physically
dedicated cable and spare Ethernet port for this session. Keep the working
internet connection on another interface. A display or serial console on the
target is needed for the temporary SSH fingerprint. The installed OS already
has its own private SSH identity, which was prepared on the workstation.

Run the following from the Linux build environment. Fill in `BOARD`, `IFACE`,
`PRIVATE_DIR` from the build and `KEY` from the workstation identity. Use a new
`BOOT_DIR` under a directory the TFTP service can read, such as a unique path
under `/tmp`. Keep the private build bundle elsewhere.

```sh
python3 scripts/install-source.py prepare --board BOARD --interface IFACE --output-dir PRIVATE_PARENT
python3 scripts/net-install.py stage --board BOARD --bundle PRIVATE_DIR --public-key KEY.pub --boot-dir BOOT_DIR
```

Configure only the dedicated interface as `10.78.0.1/24`, with no gateway.
Start the temporary DHCP/TFTP server with administrator access. Keep it in a
separate terminal or a tracked process so it can be stopped.

```sh
sudo python3 scripts/net-install.py serve --board BOARD --interface IFACE --boot-dir BOOT_DIR
```

Ask the customer to connect the direct cable, choose network boot in the target's
firmware and say **Continue**. Stop if the firmware lacks a usable network boot
option. Use the fingerprint shown on the target console. Save it to a new
`BOOT_TRUST` path in the private directory. This trust file is only for the
temporary installer.

```sh
python3 scripts/net-install.py pin --interface IFACE --fingerprint SHA256:CONSOLE_FINGERPRINT --known-hosts BOOT_TRUST
python3 scripts/net-install.py identity --interface IFACE --key KEY --known-hosts BOOT_TRUST
python3 scripts/net-install.py list --interface IFACE --key KEY --known-hosts BOOT_TRUST
python3 scripts/net-install.py inspect --interface IFACE --key KEY --known-hosts BOOT_TRUST --device DEVICE
```

Verify the model and architecture, plus CPU vendor for AMD and Intel. Show the
customer the destination path, size, model, identity, and contents. If the disk
is ambiguous, stop. Wait for the customer to type `erase DEVICE` with the actual
whole-device path. Use the identity from inspection. The write checks the disk
again and verifies the bytes read back against the payload's SHA-256.

```sh
python3 scripts/net-install.py write --board BOARD --bundle PRIVATE_DIR --interface IFACE \
  --key KEY --known-hosts BOOT_TRUST --device DEVICE --confirm-erase DEVICE --expect-identity IDENTITY
```

After a successful write, stop the DHCP/TFTP process. Have the customer power
off the target and choose the installed disk at boot. Keep the direct cable in
place for first SSH. Use the private install bundle's `known_hosts` for the
installed OS. If read-back fails, report the failure and wait for fresh erase
approval before any retry. Stop the temporary server when the session ends.
