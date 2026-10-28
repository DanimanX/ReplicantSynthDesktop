# Build

## Toolchain

- Rust 1.85 or newer (edition 2021). `rustup update stable` is enough.
- On Linux, the development headers below. macOS and Windows need none.

The workspace pins nothing else: every crate is resolved from crates.io, and
`Cargo.lock` is committed so CI and local builds agree.

## Linux packages

```bash
# Debian / Ubuntu
sudo apt-get install build-essential pkg-config libasound2-dev \
    libxkbcommon-dev libwayland-dev libx11-dev libxcb1-dev libudev-dev

# Fedora
sudo dnf install gcc pkgconf alsa-lib-devel libxkbcommon-devel \
    wayland-devel libX11-devel libxcb-devel systemd-devel

# Arch
sudo pacman -S --needed base-devel pkgconf alsa-lib libxkbcommon \
    wayland libx11 libxcb systemd-libs
```

`libasound2-dev` is for cpal's ALSA backend; the X11/Wayland packages are for
winit. A headless machine can still build and run the engine tests and the demo
renderer, which never touch a device.

## Commands

```bash
cargo build --release -p replicantsynth-desktop
cargo run   --release -p replicantsynth-desktop

cargo test  --release --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

The binary is `target/release/replicantsynth` (`.exe` on Windows).

## Feature flags

There are no optional features in 0.0.1. eframe is built with `glow` (OpenGL)
rather than wgpu, so the app runs on machines with only OpenGL 3.3 — including
most VMs and remote desktops.

## Offline rendering

```bash
cargo run --release -p replicantsynth-core --bin render-demo -- \
    --out assets/replicantsynth_demo.wav \
    --seconds 42 \
    --sample assets/demo/neon_stab.wav \
    --bpm 126
```

This opens no audio device: it builds the demo project, renders it block by block
and captures the master through the same 24-bit writer the app uses. Without
`--sample` the two sample pads are left empty.

## Headless UI capture

```bash
REPLICANTSYNTH_SCREENSHOT=/tmp/ui.png \
REPLICANTSYNTH_SCREENSHOT_FRAMES=60 \
REPLICANTSYNTH_SCREENSHOT_SCENE=0 \
cargo run --release -p replicantsynth-desktop
```

Additional knobs: `REPLICANTSYNTH_SCREENSHOT_PAD` (0–63),
`REPLICANTSYNTH_SCREENSHOT_TAB` (`sound`, `fx`, `pattern`, `mix`),
`REPLICANTSYNTH_SCREENSHOT_EDITOR=1` and `REPLICANTSYNTH_SCREENSHOT_FX=1`. On a
real desktop this needs `DISPLAY` or `WAYLAND_DISPLAY`; CI uses `xvfb-run`.

## Plugin paths

The `VST` engine scans for plugins at startup, on demand, and on **RESCAN**.
`REPLICANTSYNTH_PLUGIN_PATH` adds directories ahead of the platform defaults:

```bash
REPLICANTSYNTH_PLUGIN_PATH="$HOME/audio/plugins/clap:$HOME/audio/plugins/vst3" \
    cargo run --release -p replicantsynth-desktop
```

The host tests load whatever is installed. On a machine with no plugins they skip
themselves and print why, so a fresh CI runner stays green without pretending it
verified hosting.

See [PLUGINS.md](PLUGINS.md) for the hosting model and troubleshooting.

## Data directory

| OS | Path |
| --- | --- |
| Linux | `$XDG_DATA_HOME/ReplicantSynth` or `~/.local/share/ReplicantSynth` |
| macOS | `~/Library/Application Support/ReplicantSynth` |
| Windows | `%APPDATA%\ReplicantSynth` |

`REPLICANTSYNTH_HOME` overrides it, which is how the test suite and reviewers
keep their own projects separate. The folder holds `project.json`, `Samples/` and
`Recordings/`.

## Troubleshooting

- **"no output device"** — the UI still runs and edits; audio is offline. Pick a
  device in **SETTINGS**, then **RESTART AUDIO**.
- **Crackling** — try another sample rate in **SETTINGS**; some USB interfaces
  only run glitch-free at their native rate.
- **No MIDI ports** — on Linux the user needs to be in the `audio` group for
  ALSA raw MIDI, and the APC64 usually also needs a udev rule or a running JACK
  session for multi-cable access. **MIDI → CONNECT APC64** opens every port whose
  name contains `APC64`.
- **Wayland vs X11** — eframe picks Wayland when `WAYLAND_DISPLAY` is set and
  falls back to X11. Both are compiled in. On Wayland a plugin editor cannot be
  embedded, so it opens as its own window through XWayland; unset
  `WAYLAND_DISPLAY` to run the whole app on XWayland with editors inside the
  window. See [PLUGINS.md](PLUGINS.md#detached-wayland).
