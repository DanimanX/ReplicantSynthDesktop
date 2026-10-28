# Changelog

All notable changes to ReplicantSynth Desktop are documented here. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.5] — 2026-10-28

### Fixed

- **A hosted plugin only ran while its pad was playing.** The engine skips clips
  that are not playing and have no tail, which meant a plugin whose editor was
  open but whose pad was stopped was never processed. A plugin reports the
  parameters its own GUI moves from inside that cycle, so turning a knob in
  Bazille's editor left the *PARAMETERS* row frozen at the values it had when the
  plugin was loaded — *Output* stayed at 50% no matter what. A pad's plugin now
  keeps processing while its editor is open, and stops again when it closes; it
  stays silent throughout, because a stopped pad is still not mixed in.
- **CLAP parameter reports were thrown away.** `process()` was called with
  `OutputEvents::void()`, so a `PARAM_VALUE` event the plugin wrote on its way out
  — which is how a CLAP plugin reports an edit made in its own editor — went
  nowhere. `request_flush` was also a no-op, though CLAP requires the host to
  answer it with `params.flush()`. Both are now collected on the worker thread and
  handed to the UI.
- **Reopening a project left the parameters at their load-time values.** Replaying
  a state blob moves every parameter and reports none of them, so the panel kept
  showing what the plugin had before the state was loaded. The values are now
  re-read after a restore.

### Changed

- **A detached plugin editor no longer takes over the window.** When the editor is
  a window of its own — Wayland, or a plugin that only draws detached — the pad
  grid and the inspector stay on screen, so the parameter list is visible while
  the plugin's GUI is being used. Embedding still claims the window, where there
  is an X11 parent to embed into.

## [0.0.4] — 2026-10-28

### Fixed

- **Parameters changed in a plugin's own editor now reach the inspector.** Turning
  a knob in Bazille (or any hosted instrument) used to move the sound and nothing
  else: the parameter list the SOUND tab draws was the one read when the plugin
  was loaded, so the knobs in *PARAMETERS* stayed where they were. The host now
  pulls the plugin's values back every frame — VST3 reports the edits its editor
  makes through `IComponentHandler::performEdit`, CLAP has no such notification so
  the values are read back at ~20 Hz — and the panel follows the drag, asking for
  a repaint while it moves.

## [0.0.3] — 2026-10-28

### Fixed

- **Plugin editors on Wayland.** A Wayland surface cannot own an X11 child, so the
  plugin panel used to say *“This session has no X11 window to embed into”* and
  stop: loading a plugin left no editor at all. The host now opens a top-level
  X11 window of its own through XWayland, titles it after the plugin, and parents
  the editor to it — which is what makes plugins that only implement embedded
  editors (u-he's CLAP builds, Bazille among them) show their GUI on Wayland.
- **Detached editors stay where the desktop puts them.** The panel no longer
  repositions a floating editor to its own coordinates, and the window carries a
  minimum size hint plus `WM_CLASS` `("plugin-editor", "ReplicantSynth")` so a
  window manager can size and float it.
- **Closing a detached editor** now tears the plugin's GUI down, keeps the plugin
  playing, and returns the UI to the grid instead of leaving a window behind.

## [0.0.2] — 2026-10-28

### Added

- **VST3 and CLAP hosting**: any installed plugin can take over a pad. Choose
  **VST** in the inspector or right-click a pad, pick a plugin from a scanned
  list, and it becomes that pad's engine.
- **Plugin editors inside the window**: an embedded editor is a child X11 window
  owned by the host, sized to what the plugin reports and centred in the region
  the UI reserves. Detached windows are used where embedding is impossible
  (Wayland, macOS, Windows) or refused by the plugin.
- **CLAP timer and file-descriptor hosting** (`clap.timer`, `clap.posix-fd`), and
  VST3 run-loop servicing, so VSTGUI editors actually paint.
- **Plugin audio on its own thread**, bridged to the engine through a bounded
  block queue: a stalling plugin costs one block of audio, never the device.
- **Piano roll for plugin pads**: the same 16-step note model as ReplicantWave,
  so a phrase moves between the wavetable engine and a hosted instrument.
- **Plugin parameters as knobs** in the inspector, and **plugin state saved in
  the project** (base64 blob per pad), restored on load.
- **Plugin browser** with instruments/effects split, search and rescan.
- **Manifest-only VST3 scanning** that never executes a bundle, and CLAP scanning
  on a worker thread inside `catch_unwind`.

### Fixed

- The CLAP `posix-fd` extension is Unix-only; importing it unguarded broke the
  Windows build. It is now `cfg(unix)`, with the same behaviour on Linux.
- A host test assumed a specific plugin bundle was installed, so it failed on a
  runner without one. It now scans whatever manifests exist and asserts on those.
- The engine clamped a clip's kind to the five built-in engines, so
  `ClipKind::Plugin` silently became a sample pad.
- The plugin worker blocked on its reply when the audio thread had stopped
  reading, deadlocking shutdown; the reply is now non-blocking.
- Base64, VST3 manifest parsing and the plugin bridge protocol are covered by
  tests, including the trailing-comma dialect VST3 writes.

## [0.0.1] — 2026-10-28

First public desktop release: the Android clip launcher rebuilt as a
cross-platform Rust application.

### Added

- **64-pad clip grid** with clip state (stopped, queued, playing), kind colour,
  FX dot, per-pad peak meter and the live 16-step phase, plus an eight-row scene
  rail with launch and stop.
- **Ableton-style launching**: per-clip launch/stop toggling, global launch
  quantization (OFF, 1/16, 1/4, 1 bar, 2 bars, 4 bars), scene launch that stops
  clips outside the row, and a hard transport stop.
- **Replicant303 engine**: polyBLEP saw/square oscillator, diode ladder filter
  with saturated feedback, capacitor slide, accent envelope and the five-model
  distortion unit (Poco Rat, Tube, Hard Clip, Germanium Fuzz, Wavefolder).
- **ReplicantWave engine**: 8 wavetable banks × 16 frames × 7 mip levels with
  band-limited harmonic synthesis, frame morphing, up to 8 unison voices per
  voice, ZDF state-variable filter, sub, noise, ADSR and chord mode over an
  8-voice polyphonic allocator.
- **Replicant909 engine**: the eleven circuit-level voices (BD, SD, LT, MT, HT,
  RS, CP, CH, OH, CC, RC) with a procedurally generated 6-bit cymbal ROM,
  closed-hat choke and a linear headroom-trimmed bus behind a transparent peak
  limiter.
- **ReplicantFX engine**: all 48 SP-404MKII MFX slots with the firmware's names,
  three-character LED codes, six dynamic parameter labels and factory defaults;
  one insert per pad with smoothed parameters and an independent delay/comb bank.
- **Clip editors**: a 16-step acid sequencer with gate, accent, slide and note
  lanes plus a randomiser; an 11 × 16 kit grid with velocity-tinted cells; and a
  piano roll that writes sustained, polyphonic MIDI notes.
- **Factory content**: 512 Replicant303 lines (8 banks × 64, deterministic
  SplitMix32 generation), the 8 classic Replicant909 patterns and all 44
  ReplicantWave sounds, behind a searchable preset browser.
- **NEON GRID demo project**: six scenes, 18 pads, a bundled vocal sample and a
  per-pad FX insert on every clip, created on first launch.
- **Sample import** for WAV, FLAC, MP3, OGG/Vorbis and AAC/M4A through
  Symphonia, with loop / one-shot, gain and pitch.
- **24-bit capture**: master take or single-pad stem, streamed by a dedicated
  writer thread from a lock-free ring; takes are listed in the app and can be
  loaded straight back into a sample pad.
- **APC64 support**: Akai SysEx handshake, pad-mode and firmware detection, DAW /
  Notes cable routing, pad and scene LEDs, OLED labels, transport and record
  buttons, encoder BPM and touch-strip control.
- **Project persistence**: a versioned JSON document holding all 64 clips,
  patterns, MIDI notes, FX, mixer state and sample references, saved
  atomically with a 15-second autosave.
- **Desktop shell**: egui/eframe window with a responsive transport bar, a
  right-hand inspector (SOUND / FX / PATTERN / MIX), keyboard shortcuts, a
  settings sheet with device and sample-rate selection, and a diagnostics scope.
- **Offline tooling**: `render-demo` renders an arrangement through the real
  engine and capture path without opening an audio device, and
  `REPLICANTSYNTH_SCREENSHOT` captures the UI headlessly.
- **CI**: Linux, macOS and Windows builds with formatting, clippy, the engine
  test suite, a release build, a demo render and tag-driven GitHub releases.

### Known limits

- One ReplicantFX insert per pad; the plug-in's dual-bus routing is not exposed.
- The piano roll covers one 16-step bar per clip.
- Samples loop by length; pitch is resampling, not time-stretch.
- Standalone only: no VST3/CLAP build yet.
