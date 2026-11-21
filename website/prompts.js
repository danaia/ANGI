// Builds the one install prompt a user pastes into their coding agent.
// Phase 1: the card is in THIS computer's reader; the agent downloads, verifies
// and writes the image. Phase 2: the user moves the card into the target and
// connects a USB-C data cable; the agent finishes setup over that link.
window.ANGI_PROMPT = function (img) {
  const downloads = window.ANGI_DOWNLOADS;
  const repo = window.ANGI_REPO;
  const url = img.url || `${downloads}/${img.file}`;
  const pi = img.kind === 'pi';
  const media = img.media;
  const pin = img.sha256 ? `\nIt must also equal ${img.sha256}.` : '';

  const head = `Install ANGI on my ${img.name}. Do all the terminal work yourself on this computer.
Explain each step in one short sentence. Tell me exactly when I need to do something
physical, then wait for me. Never ask me to type a password into this chat.

There are two phases. In PHASE 1 my ${media} is in THIS computer and you write ANGI to it.
In PHASE 2 I move it into the ${img.short} and you finish setup. Say this in your first message.`;

  const phase1 = `PHASE 1 - WRITE THE ${media.toUpperCase()} ON THIS COMPUTER

1. CHECK THIS COMPUTER
Read .agents/skills/angi-install-media/SKILL.md from ${repo} before selecting an
image or writing a drive. The selected hardware target is ${img.id}.
Detect the operating system (Linux, macOS or Windows) and check for 4 GB of free space.
Ask me to put my ${media} into ${pi ? 'a card reader on' : 'a USB port of'} THIS computer${pi ? ', with the ' + img.short + ' powered off' : ''}.
Wait until it is attached before you continue.

SSH KEY (REQUIRED)
ANGI accepts SSH only with a key, so ${pi ? (img.id === 'rpizero2w' ? 'USB' : 'USB-C') + ' and Ethernet' : 'Ethernet'} access need one from
this computer. Look in my .ssh folder for an Ed25519 key pair (id_ed25519 and
id_ed25519.pub) and reuse it. If there is none, create one with
ssh-keygen -t ed25519 -f <.ssh>/angi_ed25519. Never overwrite, move or print a
private key. Record the private key's path and show me the one-line public key
(.pub). I will give it to ANGI in phase 2.

2. DOWNLOAD AND VERIFY THE IMAGE
Download ${url}
and ${url}.sha256 into a new folder. Compute the image's SHA-256 and compare it
with the .sha256 file.${pin} If the download is not found, tell me the ${img.short}
image is not published yet and stop. If anything does not match, delete the
download, tell me, and stop.

3. FIND THE ${media.toUpperCase()} AND GET MY APPROVAL
List removable disks with each one's device path, size, model and contents.
Never choose a disk that holds this computer's system. If there is more than one
candidate, ask me which. Tell me clearly that EVERYTHING on it will be erased and
wait until I type exactly: erase DEVICE (with the real device path). Anything else
means no.

4. WRITE, VERIFY AND EJECT
Write the image to the whole device, check every written byte against the image,
then eject it. Use the tool for this computer:
  Linux: clone ${repo} (no build needed) and run
    scripts/flash-sd.sh --board ${img.id} --image IMAGE --device PATH --confirm-erase PATH
    It writes, verifies and ejects.
  macOS: diskutil unmountDisk the disk, write with sudo dd to the raw device
    (/dev/rdiskN, bs=4m), read back exactly the image's length from the device
    and check its SHA-256 equals the image's, then diskutil eject.
  Windows: use Raspberry Pi Imager's command-line mode (install it with my
    approval if it is missing). It writes and verifies.
Only say it is ready once verification has passed. If anything fails, tell me and
do not try a different disk.`;

  const phase2pi = `PHASE 2 - MOVE THE CARD TO THE ${img.short.toUpperCase()} AND FINISH

5. MOVE THE CARD AND CONNECT ${img.id === 'rpizero2w' ? 'THE USB CABLE' : 'USB-C'}
Tell me Phase 1 is done, then give me these steps, one action per line:
  1) Take the card out of this computer.
  2) Push it into the ${img.short}'s microSD slot, contacts toward the board.
  3) Connect ${img.port} to this computer with a USB data cable (not
     charge-only). This one cable powers the ${img.short} and links it to this computer.
  4) Wait about a minute while it starts.
Then wait for a new USB network adapter on this computer with the address
10.77.0.1. If it does not appear within 3 minutes, help me check the cable, the
port, the power and that the card is seated.

6. FINISH SETUP IN MY BROWSER
Open http://10.77.0.2 in my browser.
Tell me to create my administrator account there, choose my Wi-Fi country, scan,
and join my network${img.id === 'rpizero2w' ? ' (the Zero 2 W only sees 2.4 GHz networks)' : ''}. I type passwords on that page, not here.
Give me the public key from the SSH key step again and tell me to paste it into
the page's "SSH public key" field. The page marks it optional; for this install it
is required. Without it I cannot connect over SSH, USB or Ethernet.
When the page shows "Setup complete" I will say "Continue". Ask me to paste the
result lines from the page (they contain no passwords). Confirm that Wi-Fi and the
internet check passed.

7. PIN THE PI'S IDENTITY AND CHECK SSH
The result lines include "Pi SSH host key: SHA256:...". Write a known_hosts file
just for this Pi. Fill it only with an Ed25519 host key whose fingerprint equals
that line, under the names 10.77.0.2 and 10.78.0.2. Then run, with my private key and
strict host key checking: ssh MY-NAME@10.77.0.2 agx status --json. If the
fingerprint differs, stop and tell me; never turn host key checking off. Finally
show me the ssh command and its options, and tell me the same key and pinned
identity work over Ethernet: the Pi is 10.78.0.2, this computer 10.78.0.1/24 with no
gateway. The USB cable stays a private link to this computer for development;
the ${img.short} reaches the web over Wi-Fi.`;

  const phase2desktop = `PHASE 2 - MOVE THE DRIVE TO THE ${img.short.toUpperCase()} COMPUTER AND FINISH

5. MOVE THE DRIVE AND START FROM IT
Tell me Phase 1 is done, then give me these steps, one action per line:
  1) Unplug the USB drive from this computer.
  2) Plug it into the ${img.short} computer and power it on.
  3) Open its boot menu (often F12, F11 or Esc) and choose the USB drive.
Help me find the boot menu key or the firmware setting for USB boot if needed.

6. FINISH SETUP
Read the setup steps in the release notes for this image at
${window.ANGI_RELEASES}/latest and walk me through them, one step at a time.
When setup asks for an SSH public key, give me the one from the SSH key step to
paste. It is required for this install: without it I cannot connect from this
computer over Ethernet. Pin the ${img.short} computer's host key only after
checking its fingerprint against the one setup shows, and keep strict host key
checking on.`;

  return [head, phase1, pi ? phase2pi : phase2desktop].join('\n\n');
};
