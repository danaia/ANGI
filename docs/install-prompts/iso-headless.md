# Install ANGI headless from an existing Pi 4 / 400 ISO

Use this prompt with an agent on the Linux computer connected to the card reader.
Provide the generic `angi-rpi4.iso` and its matching release `.sha256` file.
For the primary build-and-write workflow, see the [Pi 4 / 400 provisioning guide](../pi4-provisioning.md).

```text
Install ANGI headless on my Raspberry Pi 4 or 400 using the generic release ISO.
Use the ANGI checkout at https://github.com/danaia/ANGI.git for the install scripts.
Read .agents/skills/angi-install-media/SKILL.md before selecting or writing the image.
Do the terminal work on this Linux computer and explain each step in one short sentence.
Tell me when I need to do something physical and wait for me.

1. CHOOSE THE SETUP FLOW
Use USB-C browser setup by default. Ask me to choose SSH setup if I need direct
Ethernet as a fallback or want the agent to apply the Wi-Fi settings itself.

2. PREPARE THE IMAGE
Find my generic angi-rpi4.iso and the matching published SHA-256 from the trusted
release, and verify the downloaded file before proceeding. For browser setup, use
the generic ISO directly. For SSH setup, check for xorriso, bsdtar, debugfs,
dropbearkey, OpenSSH, Python 3.11 or later, and util-linux. Install missing tools
with my approval if needed.
For SSH setup, run python3 scripts/provision-iso.py --image ISO --sha256 SHA256.
It reuses or creates the workstation key pair and creates the Pi's per-card host
key. Keep the resulting install directory private. Record the workstation identity
path and report the Pi host-key fingerprint.
Use the private angi-rpi4-install.iso and its known_hosts file for the rest of setup.

3. WRITE THE CARD
Run scripts/flash-sd.sh --list and show me the card path, size, model, serial, and contents.
Confirm the card is attached to this computer and holds no system mounts.
Tell me that everything on the selected card will be erased.
Wait for me to type the exact approval, for example erase /dev/sdb.
Run scripts/flash-sd.sh --board rpi4 --image ISO --device PATH --confirm-erase PATH
with the generic ISO for browser setup or the private ISO for SSH setup.
Wait for successful verification and ejection before telling me the card is ready.
Tell me if anything fails and keep the selected device unchanged.

4. BOOT AND SET UP
Ask me to put the ejected card into the powered-off Pi and connect a USB-C data cable
to this computer, or use a direct Ethernet cable and the Pi's power supply.
For browser setup, wait for the workstation to get its USB address, then open
http://angi.local or http://10.77.0.2. Let me enter the administrator and Wi-Fi
details there. After entering the country code, use Scan nearby networks and
select the Wi-Fi name, or enter the SSID for a hidden network.
Check the result and report the Wi-Fi address, the DNS and HTTPS
result, and whether the page has shut down. The password stays in the browser form.
For SSH setup, use the chosen cable to reach the Pi. For USB-C, use workstation 10.77.0.1/24 and Pi
10.77.0.2. For Ethernet, use workstation 10.78.0.1/24 and Pi 10.78.0.2. Configure
only the dedicated adapter, with no gateway and no default route, and verify the path.
Connect as root with my SSH private key and StrictHostKeyChecking=yes, IdentitiesOnly=yes,
and UserKnownHostsFile pointing to the prepared known_hosts file.
Confirm the model in /proc/device-tree/model and run agx status --json.

5. JOIN WI-FI AND FINISH (SSH FLOW)
Ask me for the Wi-Fi network name, two-letter country code, and security type.
Ask whether the network is hidden if it does not appear in a scan.
Set the country with agx wifi country CODE and scan with agx wifi scan --json.
Open a workstation terminal with SSH and run agx wifi connect for the chosen network.
Let me type the password at the Pi's hidden prompt. Keep it out of chat and logs.
Verify association and IP with agx wifi status --json, then DNS and HTTPS with agx wifi check.
Ask which account I want, use the Profile tab in agx tui to set it up, and let me enter
its password in the terminal. Verify agx services --json and tell me how to reconnect
and shut down safely. Report any remaining setup and report success only once Wi-Fi works.
```
