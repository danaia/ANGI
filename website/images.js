// ANGI image manifest: the single place to publish a download.
//
// The install prompts build from source. To offer an optional release image,
// upload the image and its .sha256 file, then set status to 'available' here.
// The Install page can display its size and SHA-256.
window.ANGI_RELEASES = 'https://github.com/danaia/ANGI/releases';
window.ANGI_DOWNLOADS = 'https://github.com/danaia/ANGI/releases/latest/download';
window.ANGI_REPO = 'https://github.com/danaia/ANGI.git';

window.ANGI_IMAGES = [
  {
    id: 'rpi5',
    name: 'Raspberry Pi 5',
    short: 'Pi 5',
    icon: 't-pi5',
    blurb: 'The fastest Pi. Full desktop on HDMI, optional NVMe for data.',
    kind: 'pi',
    media: 'microSD card',
    port: "the Pi 5's USB-C port",
    file: 'angi-rpi5.img',
    status: 'soon',
    version: null, sha256: null, size: null,
  },
  {
    id: 'rpi4',
    name: 'Raspberry Pi 4 / 400',
    short: 'Pi 4 / 400',
    icon: 't-pi4',
    blurb: 'Desktop ANGI with setup over USB-C or Ethernet.',
    kind: 'pi',
    media: 'microSD card',
    port: "the Pi's USB-C port",
    file: 'angi-rpi4.iso',
    status: 'soon',
    version: null, sha256: null, size: '1.2 GB',
  },
  {
    id: 'rpizero2w',
    name: 'Raspberry Pi Zero 2 W',
    short: 'Pi Zero 2 W',
    icon: 't-zero',
    blurb: 'Tiny, quiet and always on. Reaches the web over 2.4 GHz Wi-Fi.',
    kind: 'pi',
    media: 'microSD card',
    port: 'the micro-USB port marked USB (not PWR IN)',
    file: 'angi-rpizero2w.img',
    status: 'soon',
    version: null, sha256: null, size: null,
  },
  {
    id: 'arm-desktop',
    name: 'ARM computer',
    short: 'ARM',
    icon: 't-arm',
    blurb: 'UEFI laptops and desktops on ARM. Starts from a USB drive.',
    kind: 'desktop',
    media: 'USB drive',
    port: null,
    file: 'angi-arm64.iso',
    status: 'soon',
    version: null, sha256: null, size: null,
  },
  {
    id: 'x86-desktop',
    name: 'Intel / AMD computer',
    short: 'Intel / AMD',
    icon: 't-x86',
    blurb: 'Experimental build for x86-64 UEFI computers. Starts from a USB drive.',
    kind: 'desktop',
    media: 'USB drive',
    port: null,
    file: 'angi-x86_64.iso',
    status: 'soon',
    version: null, sha256: null, size: null,
  },
];
