# ANGI Workspace 2 graphics performance

November 24, 2026

**Both graphics stacks use hardware acceleration. Raising the Intel GPU's minimum render frequency from 200 to 400 MHz brought the tested Workspace 2 drag to the display's 60 Hz cadence.** The workstation at `10.78.0.1` uses a Ryzen 7 5700U. The ANGI `winit-femtovg` preview and an independent EGL probe both reported RadeonSI on the Radeon GPU, with `amdgpu` bound. The verified Beelink Mini S at `10.78.0.2` uses an Intel Celeron N5095. It binds the Intel UHD GPU (`8086:4e55`) to `i915`, and the running ANGI `linuxkms-femtovg` desktop reports `Mesa Intel(R) UHD Graphics (JSL)` from its own OpenGL context at 1920 × 1080. The active HDMI mode is 1920 × 1080 at exactly 60 Hz. [Intel and AMD first-boot procedure](install-prompts/pc-uefi.md)

## How the desktop is built

- The GUI lockfile resolves Slint **1.19.0** and FemtoVG **0.28.0**. Buildroot's desktop package selects the `kms` feature for normal appliance builds. Native mode uses Slint → FemtoVG OpenGL → Mesa EGL/GBM → DRM/KMS when ANGI selects `linuxkms-femtovg`. The standalone Full Desktop session runs through Weston and should be measured separately. ANGI can also select Slint's CPU renderer explicitly or use `vnc-software` for a headless session. [GUI features](../crates/agx-gui/Cargo.toml) · [Buildroot desktop package](../buildroot/package/agx-gui/agx-gui.mk) · [Backend selection](../crates/agx-gui/src/graphics.rs) · [Slint LinuxKMS](https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backend_linuxkms/)
- The measurements used a Beelink-specific image. The generic x86 Buildroot configuration selects the ANGI desktop with software rendering for setup. It can switch to the accelerated graphics stack after provisioning downloads the matching bundle. It loads the provisioned `i915` module after the root filesystem is mounted so Intel firmware is available when the driver initializes. Verify the shipping image's kernel binding, Mesa driver and actual GL renderer on each hardware target. For an Iris-supported Intel GPU, look for the Intel hardware renderer in the desktop's GL context. A FemtoVG backend using Mesa's `llvmpipe` still rasterizes on the CPU. [Generic x86 defconfig](../buildroot/configs/agx_x86_desktop_defconfig) · [Buildroot PC kernel configuration](https://gitlab.com/buildroot.org/buildroot/-/blob/2026.08.1/board/pc/linux.config) · [Mesa driver source tree](https://docs.mesa3d.org/sourcetree.html) · [Mesa LLVMpipe](https://docs.mesa3d.org/drivers/llvmpipe.html)

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

### Intel native desktop window motion

I replayed the same 120 Hz pointer gesture through the authenticated VNC tunnel without requesting VNC frames during sampling. The desktop rendered to physical HDMI through FemtoVG, EGL and Intel DRM/KMS. I polled the primary CRTC's framebuffer ID and timed observed buffer changes. The table pools the central four seconds from each of two five-second drags per comparison, with roughly 4–6 ms maximum polling gaps. The live verification uses 15.5 seconds from a separate 17-second drag. This measures desktop framebuffer change cadence, with polling jitter. It does not measure panel response or physical input-to-photon latency. A gap above 25 ms is consistent with a missed 60 Hz refresh interval. [Linux DRM/KMS display pipeline](https://docs.kernel.org/gpu/drm-kms.html#kms-display-pipeline)

| Workload and runtime policy | Changes/s | p50 gap | p95 gap | p99 gap | Gaps over 25 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Workspace 1 dashboard, stock | 51.94 | 16.02 ms | 32.07 ms | 36.04 ms | 64 / 413 |
| Workspace 2 Agent Angi, Web collapsed, stock | 57.75 | 16.01 ms | 20.06 ms | 35.98 ms | 18 / 459 |
| Workspace 2 Agent Angi, Web visible, stock | 54.58 | 16.01 ms | 32.00 ms | 36.07 ms | 43 / 434 |
| Same scene, CPU performance governor | 56.98 | 16.02 ms | 31.87 ms | 35.96 ms | 24 / 454 |
| Same scene, CPU performance and 400 MHz GPU floor | 60.01 | 16.02 ms | 19.98 ms | 20.05 ms | 0 / 477 |
| Same scene, 400 MHz GPU floor, stock CPU policy | 59.99 | 16.01 ms | 20.00 ms | 20.06 ms | 0 / 478 |
| Same scene, longer verification after applying the 400 MHz floor | 60.00 | 16.01 ms | 20.00 ms | 20.05 ms | 0 / 929 |

The **400 MHz GPU floor** matched the combined CPU and GPU setting on these runs. ANGI still uses the stock `intel_pstate` CPU policy and the 750 MHz GPU ceiling. The kernel resets the runtime setting on reboot. The ANGI KMS greeter now reapplies the measured floor on this exact board when it sees the factory default and a connected display. The thermal readings were unchanged at the start and end of the 17-second drag. This test covers window motion with Web visible. Transcript scrolling, resizing, other resolutions and the remote viewer's own cadence have their own workloads.

The kernel also reported that it could not find `i915/icl_dmc_ver1_09.bin` at driver initialization, so it disabled display runtime power management. The file is present on the mounted root filesystem and the running kernel has `i915` built in. Including the firmware in the kernel or early boot firmware payload would make it available at initialization. The clock-floor A/B test was performed with the same firmware state throughout. [Intel DMC firmware support](https://docs.kernel.org/gpu/intel-display/dmc.html)
