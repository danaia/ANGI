# ANGI Workspace 2 graphics performance

November 24, 2026

**The Intel desktop's GPU acceleration and frame pacing are still unverified.** The workstation available to this session has a Ryzen 7 5700U with Radeon graphics and runs the `amdgpu` kernel driver. The Ethernet peer at `10.78.0.2` responded to all five pings (0.33 ms average), but its SSH host key differs from the saved device identity. The peer's console fingerprint or its private provisioning record is needed to verify the target. The ping result measures the direct link only. [Intel and AMD first-boot procedure](install-prompts/pc-uefi.md)

## How the desktop is built

- The GUI lockfile resolves Slint **1.19.0** and FemtoVG **0.28.0**. Buildroot's desktop package selects the `kms` feature for normal appliance builds. Native mode uses Slint → FemtoVG OpenGL → Mesa EGL/GBM → DRM/KMS when ANGI selects `linuxkms-femtovg`. The standalone Full Desktop session runs through Weston and should be measured separately. ANGI can also select Slint's CPU renderer explicitly or use `vnc-software` for a headless session. [GUI features](../crates/agx-gui/Cargo.toml) · [Buildroot desktop package](../buildroot/package/agx-gui/agx-gui.mk) · [Backend selection](../crates/agx-gui/src/graphics.rs) · [Slint LinuxKMS](https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backend_linuxkms/)
- The checked-in generic x86 Buildroot defconfig enables neither the ANGI GUI package nor Mesa. The upstream PC kernel configuration includes `i915`, which covers kernel-side display support on applicable Intel GPUs. The Intel desktop image must supply a GUI configuration beyond the generic defconfig. Verify the shipping image's kernel binding, Mesa driver and actual GL renderer before deciding whether it uses the GPU. For an Iris-supported Intel GPU, look for the Intel hardware renderer in the desktop's GL context. A FemtoVG backend using Mesa's `llvmpipe` still rasterizes on the CPU. [Generic x86 defconfig](../buildroot/configs/agx_x86_desktop_defconfig) · [Buildroot PC kernel configuration](https://gitlab.com/buildroot.org/buildroot/-/blob/2026.08.1/board/pc/linux.config) · [Mesa driver source tree](https://docs.mesa3d.org/sourcetree.html) · [Mesa LLVMpipe](https://docs.mesa3d.org/drivers/llvmpipe.html)

## What could cause the skips

| Symptom | First check |
| --- | --- |
| All workspaces look choppy on the physical monitor | Verify the running renderer and active display mode. A 30 Hz mode presents at most 30 frames/s. A 4K frame has four times the pixels of 1080p, so test at 1080p with the same refresh rate. |
| Local HDMI is smooth, but the VNC viewer skips | ANGI's mirror polls every 33 ms and spaces captures at least 66 ms apart while input is recent, a nominal ceiling of about 15 captures/s. A FemtoVG mirror snapshot also renders to an offscreen texture and reads the pixels back. Compare the same physical HDMI gesture with the viewer disconnected and connected to isolate capture overhead. |
| Workspace 2 lags while the same gesture is smooth elsewhere | Compare an empty Chat with a long transcript. Chat lays out the entire transcript in a `Flickable` with a repeated, wrapped `TextInput` per message. Its layout can get more expensive as history grows, even when the GPU accelerates drawing. The history drawer already uses a `ListView`. For OS or Project mode, compare with and without an active Agent run to isolate workload contention. |

[DRM mode selection](../crates/agx-gui/vendor/i-slint-backend-linuxkms/drmoutput.rs#L597) · [VNC capture loop](../crates/agx-gui/src/remote/mod.rs#L155) · [Slint OpenGL snapshot implementation](https://github.com/slint-ui/slint/blob/v1.19.0/internal/renderers/femtovg/opengl.rs#L345) · [Agent transcript layout](../crates/agx-gui/ui/agent-chat.slint#L109) · [Agent history drawer](../crates/agx-gui/ui/agent-history.slint#L73)

## Metrics

### Local VNC software baseline

I ran the existing ANGI debug preview on the AMD workstation with `SLINT_BACKEND=vnc-software`, an isolated profile and a loopback RFB client. The client alternated desktop sizes, timed each resize request through receipt of the complete raw pixel frame, and verified the response dimensions. I discarded four warm-up requests and used nearest-rank p95 for the remaining 40 requests per resolution.

| Requested size | Median | p95 | Maximum |
| --- | ---: | ---: | ---: |
| 1280 × 720 | 291 ms | 422 ms | 430 ms |
| 1920 × 1080 | 547 ms | 582 ms | 659 ms |

This baseline includes resizing, CPU rendering, and raw VNC frame transfer. It measures the cost of switching virtual display size in a debug build. Steady-state scrolling, physical HDMI presentation and Intel GPU performance require the target run.

### Intel display benchmark

The primary Intel metric is **p95 time between newly presented frames during continuous Workspace 2 input**, alongside p50, p99 and the percentage of display refresh opportunities missed. A 60 Hz display gives each frame 16.67 ms. Two refresh periods produce a 33.33 ms gap, which an FPS average can hide. For a 60 Hz baseline, aim for at least 95% of active intervals within one refresh period, allowing a small tolerance for clock jitter. Also record input-to-first-visible-change latency for workspace switches and typing.

Capture at least three repeat runs of Workspace 1 and Workspace 2 with the same display mode, build and input sequence. Use empty and long-chat fixtures for scroll and typing, then repeat the reported gesture with the VNC viewer connected if applicable. Record the GPU PCI ID, kernel driver, actual GL renderer, output resolution and refresh, and process CPU and GPU activity. Use presentation timestamps from DRM page flips in Native mode or Weston’s timeline in Full Desktop. Slint's `SLINT_DEBUG_PERFORMANCE=refresh_lazy,console` provides a secondary rendered-FPS count. Measure model response time separately from interaction latency. [Slint performance diagnostics](https://docs.slint.dev/latest/docs/slint/guide/development/debugging_techniques/) · [ANGI display and Weston diagnostics](desktop.md#rendering-and-performance)
