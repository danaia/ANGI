# AGX changes to i-slint-backend-linuxkms 1.19.0

Vendored from crates.io and patched through `[patch.crates-io]` in `crates/agx-gui/Cargo.toml`.

- `hwcursor.rs` (new): shows the pointer on the DRM hardware cursor plane (legacy cursor ioctls on
  a dumb buffer) and exposes `hwcursor::move_to` / `hwcursor::available`. Falls back to the stock
  in-frame cursor when there is no cursor plane, with `SLINT_KMS_ROTATION`, on any ioctl error, or
  when `AGX_SOFTWARE_CURSOR` is set.
- `drmoutput.rs`: calls `hwcursor::install` once the CRTC is chosen.
- `fullscreenwindowadapter.rs`: `set_mouse_cursor` hands the shape to the hardware plane first.
- `cursors/*.svg`: copies of `ui/cursors/*.svg` for the move and resize shapes.

Everything else is unchanged; drop the patch when upstream gains cursor-plane support.
