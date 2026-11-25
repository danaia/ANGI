# First boot on a 64-bit UEFI computer

Use a USB stick or an SD card in a USB reader. Power the target normally and
choose the device in the firmware's one-time external boot menu. If the loader
is rejected by the firmware policy, stop and tell the customer which setting
or hardware is blocking boot. Keep the installed internal system intact.

For the shared Intel and AMD x86-64 image, use the on-screen setup. Connect to Wi-Fi
or connect Ethernet to the router, then follow **Prepare drivers** before
creating the administrator account. Choose **Restart and check drivers**
when setup installs a bundle. If a display driver prevents the desktop from
starting, select **ANGI (graphics recovery)** in the boot menu.

For private workstation-assisted media, follow the direct SSH procedure
below. After moving the prepared media, connect the target's Ethernet port to a
dedicated workstation Ethernet interface and ask the customer to say
**Continue**. Configure that interface as `10.78.0.1/24` with no gateway or
default route. ANGI listens at `10.78.0.2`. Leave the existing internet route alone.

A private copy prepared from the generic x86 image still starts with on-screen
setup and wired SSH closed. It carries the workstation's public key and its
own host key. Choose **Open Ethernet** on the Security screen. Use the copy's
`known_hosts` file on the workstation to connect with the pinned key. Inbound
SSH on this private copy is limited to the wired links you open.

Connect as root using the workstation identity and the private install
directory's `known_hosts`. Use `IdentitiesOnly=yes`, `StrictHostKeyChecking=yes`
and `HostKeyAlias` from the selected board profile. Stop if the host key
changes. Check `uname -m`, the target model, `/sys/firmware/efi`, and
`agx status --json`. On AMD and Intel check the `vendor_id` in
`/proc/cpuinfo`. Check that the running root partition belongs to the
approved external media by inspecting `findmnt -n -o SOURCE /` and `lsblk`.

Open `agx tui` using an interactive SSH session. Let the customer set the
hostname and administrator password in Profile. If the administrator has no
login key, install the workstation public key under that account with the
correct ownership and SSH permissions, preserving existing keys. Verify
administrator SSH using the same pinned host identity. For supported wireless
hardware, set the country and use **Network → Connect & save**. Confirm the
address and DNS/HTTPS using `agx wifi status --json` and `agx wifi check`.
For wired internet use a separate interface or switch the dedicated interface
only after confirming a new route. If the available network has no compatible
adapter, keep direct SSH and report the setup step still pending.
