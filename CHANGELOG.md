# Changelog

All notable changes to ReplicantSynth Desktop are documented here. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
