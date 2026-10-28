# Architecture

ReplicantSynth Desktop is a single-process Rust application with a hard split
between UI state, control messages and the real-time audio domain. It is a port
of the Android engine described in
[ReplicantSynthAndroid/docs/ARCHITECTURE.md](https://github.com/DanimanX/ReplicantSynthAndroid/blob/main/docs/ARCHITECTURE.md),
restructured so the DSP is one crate with no I/O and the desktop shell is another.

## Crates

```
crates/replicantsynth-core     engine: DSP, clip model, JSON store, capture
crates/replicantsynth-desktop  app: window, audio device, MIDI, sample decoding
```

`replicantsynth-core` never opens a device, a file dialog or a window. That is
what lets the regression suite and the offline renderer run on a CI runner with
no sound card.

```
┌─────────────────────────────────────────────────────────────┐
│ egui / eframe  (UI thread)                                   │
│  App · PadGrid · Inspector · ClipEditor · Sheets             │
│  project document, presets, sample import, MIDI events       │
└───────────────▲──────────────────────────────┬──────────────┘
                │ atomics / telemetry          │ Commands
┌───────────────┴──────────────────────────────▼──────────────┐
│ EngineHandles                                                │
│  ArrayQueue<Command> · Telemetry · SampleSlots (ArcSwap)     │
│  RecordTap (lock-free ring)                                  │
└───────────────▲──────────────────────────────┬──────────────┘
                │ audio callback               │ writer thread
┌───────────────┴──────────────────────────────▼──────────────┐
│ ClipEngine                                                   │
│  transport · 64 clips · 4 engines · 48 FX · 2 captures       │
└───────────────▲─────────────────────────────────────────────┘
                │ float stereo, device rate
┌───────────────┴─────────────────────────────────────────────┐
│ cpal → ALSA · CoreAudio · WASAPI · JACK                      │
└─────────────────────────────────────────────────────────────┘
```

## Threading

**UI thread.** Owns `ProjectState` (the `Clip` vector), the selected pad, open
windows, presets and sample decoding. It never reads or writes DSP memory
directly and never blocks on the audio thread.

**MIDI callback threads.** `midir` delivers raw bytes; an incremental parser
turns them into `MidiMessage`s on a channel. The UI thread drains the channel
once per frame and translates events into commands, or into UI actions (pad
selection, transport, recording).

**Audio callback.** Owns all DSP state: voices, envelopes, filters, FX delay
lines, clip phases, transport counters and meters. It performs no allocation,
no locking, no file I/O and no callbacks into the UI.

**Command ring.** `Configure`, `Params`, `Step`, `DrumStep`, `MidiNote`,
`Preview`, `Launch`, `Stop`, `Launch/StopScene`, `StopAll`, `Clear` and `Panic`
are copied into a fixed `ArrayQueue<Command>` (8192 entries). `Command` is a
plain `Copy` struct with the full 32-float parameter block inline, so the audio
thread never takes ownership of anything. A full queue drops the newest command
rather than blocking; the UI re-sends authoritative state on the next full sync.

**Telemetry.** One atomic per pad packs kind (4 bits), state (4), phase (8), FX
type (8), mute and solo. Pad meters, the master meter, a 512-sample scope and the
transport step are separate atomics, polled at the UI frame rate.

**Samples.** The UI decodes a file into an interleaved `f32` buffer, wraps it in
`Arc<SampleData>` and publishes it into an `ArcSwapOption` slot for that pad. The
audio thread only loads the pointer; a replaced sample is dropped on the UI side
later, never under the callback.

**Capture.** Two `RecordTap` rings (master and one stem) are pushed by the audio
thread and drained by `RecordingWriter` threads that write little-endian 24-bit
PCM and patch the WAV header on stop. The audio thread never opens a file.

## Clip engine

### Grid

64 pads are independent clip slots, mapped row-major from the top-left. Each slot
stores its engine kind, root note, 16-step pattern, drum masks and velocities,
MIDI notes, synth parameters, FX type and six FX macros, mixer gain/pan/mute/solo.

### Timing

- 16th-note grid derived from BPM: `samplesPerStep = sampleRate * 60 / bpm / 4`.
- Swing shortens even steps and lengthens odd steps by ±swing/2.
- Launch quantization is OFF, 1/16, 1/4, 1 bar (16 steps), 2 bars or 4 bars. A
  launch schedules `pending_start` at the next boundary; a second press schedules
  `pending_stop`. The UI sees `QueuedStart` / `QueuedStop`.
- Scenes launch every pad in a row and stop clips outside that row. Stopping a
  scene stops the row only.
- Transport stop is a hard stop; a per-clip stop leaves a 0.75 s FX/release tail
  that fades out linearly so delay and reverb tails are not cut.

### Engines

| Kind | Implementation | Notes |
| --- | --- | --- |
| 303 | `dsp::acid` | polyBLEP oscillator, diode ladder filter, capacitor slide, accent envelope, five distortions. |
| Wave | `dsp::wavetable` | 8 banks × 16 frames × 7 mips, shared read-only tables, 8-voice poly per pad, up to 8 unison per voice, ZDF SVF, sub/noise, ADSR, chord mode. |
| 909 | `dsp::drums` | 11 circuit-level voices; 6-bit cymbal ROM generated once into a `OnceLock`; CH→OH choke. |
| Sample | `ClipEngine::render_chunk` | Stereo float, linear interpolation, source-rate ratio, pitch, loop or one-shot. |

### Wavetable banks

Each bank frame is defined by a harmonic amplitude/phase table (`bank_harmonics`),
which is then rendered into 7 mip levels of 1024 samples with a 4096-entry sine
table. Mip selection follows the note frequency, so a high note reads a table
band-limited to Nyquist. The eight banks are built in parallel once, behind a
`OnceLock`, and are read-only afterwards.

### MIDI notes (ReplicantWave)

Wave clips carry a second pattern model: up to 96 `MidiSlot` entries with
`start`, `length`, `note` and `velocity`, measured in 16th-note steps.
`trigger_midi_step` releases notes whose end step matches the current step and
then starts notes anchored on it. `WaveSynth` starts one voice per key (or three
in chord mode) and releases voices by their root key, so overlapping notes stay
polyphonic. Clips with no MIDI notes keep the legacy 16-step gate pattern, which
the editor converts to notes the first time the piano roll opens.

### 909 gain staging

Each voice keeps its own VCA / op-amp saturation, but the eleven voices are
summed into a linear bus with a fixed 0.75 trim. The previous `tanh` on the sum
compressed every transient (crest factor ≈ 1.7); the linear bus restores a crest
factor above 3, and a transparent `soft_limit()` only bends peaks above 0.9. The
same limiter replaces the master `tanh`, so a single clip is not pre-distorted by
the safety stage.

### FX

`dsp::fx::effect` ports the ReplicantFX `Sp404BusProcessor` match arms for all 48
SP-404MKII slots: Filter+Drive, Isolator, Crusher, Phaser, Chorus/Flanger/JUNO,
drives, seven delay variants, reverbs, resonator, DJFX looper family,
tremolo/slicer, wah/super filter, EQ, vinyl/cassette, ring/pitch/sub family and
compressor. Parameters are smoothed, and each clip owns an independent delay line
and comb bank. Memory is lazy in the sense that a `Clip` allocates its FX delay
lines once at engine construction, on the UI thread, before the callback runs.

## Desktop shell

**Audio.** `audio::start_audio` picks the requested device (or the default),
reads its preferred sample format and builds a typed cpal stream (`f32`, `i16`,
`u16`, `i32`, `f64`). The engine is moved into the callback; the UI keeps only
`EngineHandles`. Changing device or rate builds a new engine and re-syncs the
project into it.

**Sample decoding.** `decode::decode_file` uses Symphonia to probe the container
and decode the first audio track, converting every sample format to interleaved
stereo `f32`.

**MIDI.** `midi::MidiHub` opens one `MidiInput` per matching port (APC64 exposes
DAW, Notes, MIDI and Custom cables as separate ports), tags events with the port
index, and keeps one `MidiOutput` for the handshake, LEDs and OLED. The parser is
incremental and honours running status, so a message split across USB packets
still decodes.

**UI.** `App` holds the project and the handles. `update()` reads telemetry,
drains MIDI, pushes APC64 feedback at ~10 Hz, autosaves, then draws: a top
transport panel, a bottom status panel, a right inspector panel and the pad grid
in the centre. Windows (editor, presets, FX picker, MIDI, takes, project,
settings, pad menu, diagnostics) are drawn last.

**Headless capture.** When `REPLICANTSYNTH_SCREENSHOT` is set, the app renders a
few frames, asks egui for a screenshot, writes the PNG and exits. CI uses this to
prove the UI builds a real frame.

## Testing

`crates/replicantsynth-core/tests/engine.rs` renders audio for every claim it
makes: the wavetable banks are checked for range and continuity, the 303 voice
for life and boundedness, every FX slot for finite and bounded output, the clip
engine for launch/stop quantization, and the recorder by reading the WAV header
back.

The desktop crate tests the parts that do not need a device or a window: pad and
note mapping, the Akai SysEx envelope, pad-mode and firmware parsing, the
incremental MIDI parser (running status, split packets, zero-velocity note-on),
the LED palette, port-role resolution, and Symphonia decoding of every container
the import dialog advertises. The audio stream and the window are exercised by
running the app, which CI does headlessly under Xvfb and captures as a PNG.

Capture has two modes, and the difference is load-bearing:
`RecordingWriter::start(.., blocking)` takes `false` from the app, where the
audio callback must never wait and dropping frames is the only safe option, and
`true` from the offline renderer and the tests, where the producer outruns the
disk and a take must be complete.
