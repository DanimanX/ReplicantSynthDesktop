# Plugin hosting (VST3 / CLAP)

ReplicantSynth Desktop can put an installed VST3 or CLAP plugin on any pad. The
plugin then behaves like a built-in engine: the piano roll plays it, its audio
goes through the pad's mixer strip and FX insert, its level shows on the meter,
and the master capture records it.

## Choosing a plugin

**VST** in the inspector's engine row, or right-click a pad → **PLUGIN**, opens
the browser. It scans on a worker thread, so the window stays responsive, and
lists what it found, split into instruments and effects.

The scan roots, in order:

1. `REPLICANTSYNTH_PLUGIN_PATH` (colon-separated on Unix, semicolon on Windows)
2. Platform defaults:
   - **Linux**: `/usr/lib/clap`, `/usr/local/lib/clap`, `~/.clap`, `/usr/lib/vst3`,
     `/usr/local/lib/vst3`, `~/.vst3`, plus the Flatpak export directories
   - **macOS**: `/Library/Audio/Plug-Ins/CLAP`, `/Library/Audio/Plug-Ins/VST3` and
     the same two under `~/Library`
   - **Windows**: `%PROGRAMFILES%\Common Files\CLAP` and `...\VST3`

The walk descends three levels, which covers vendor subfolders.

## What happens when you pick one

1. The backend instantiates the plugin and enumerates its parameters.
2. It activates the plugin for the current sample rate and a 1024-frame maximum.
3. The audio processor moves to its own thread; the engine receives only a
   bridge to it.
4. The pad becomes a `Plugin` clip with an empty piano roll, and the editor opens.

A failed load leaves the pad exactly as it was, and reports why.

## Audio path

The plugin never runs on the audio callback. The engine pushes 128-frame blocks
into a bounded three-deep queue and reads back the block the worker finished one
step earlier:

```
audio callback          worker thread
  fill block ─────────▶ PluginWorker::process()
  read previous block ◀─────── transformed block
```

That costs one block of latency (≈2.7 ms at 48 kHz) and buys a hard guarantee:
a plugin that blocks, crashes its thread, or simply runs slow can only ever
starve one block. The bridge counts drops and underruns, so a misbehaving plugin
is visible rather than mysterious.

Sends in both directions are non-blocking. The worker's reply uses `try_send`,
because a full queue means the audio thread stopped reading — blocking there
would deadlock shutdown.

## Notes

Plugin clips use the same piano-roll model as ReplicantWave: up to 96 notes with
start, length, velocity, all in 16th-note steps. `trigger_plugin_step` sends
note-on and note-off events to the bridge, which stamps them with a sample offset
inside the block. Stopping a clip releases every note it is holding, so a pad
never hangs.

Preview notes from the APC64, the on-screen keyboard or the piano roll reach the
plugin too.

## Editors

### Embedded (X11)

The host creates a child X11 window inside the application window and hands its
id to the plugin. The child inherits the parent's visual and depth — asking X for
a default visual when the app's GL surface is 32-bit is what produces `BadMatch`
on creation.

The plugin is then expected to drive its own input handling. Two things are still
the host's job:

- **CLAP**: `clap.timer` and `clap.posix-fd`. Most CLAP instruments use VSTGUI,
  which only paints once the host ticks its timers and reports readiness on its
  X11 file descriptor. The host implements both extensions, pumps timers every
  frame while an editor is open, and `poll()`s the registered descriptors
  without blocking.
- **VST3**: `IComponentHandler`'s run loop. `service_run_loop()` is called every
  frame; without it a VSTGUI editor shows a blank rectangle.

The editor is sized to what the plugin reports and centred in the region the UI
reserves. A fixed-size editor keeps its size; the frame follows it.

### Detached

When embedding is impossible the plugin creates a top-level window of its own.
That covers Wayland (a `wl_surface` cannot parent another window), macOS and
Windows in 0.0.2, and any plugin that refuses `set_parent`.

## Parameters

The inspector shows the first 64 parameters as knobs, labelled by the plugin.
Values are normalized 0..1 in both formats. Changes travel to the worker on a
lock-free queue and are applied as CLAP parameter events, or `setParameter` calls
for VST3, before the next block.

A plugin with thousands of parameters (Dexed reports 2238) is meant to be edited
in its own editor; the knob row is for quick access to the first few.

## State

Saving a project asks each loaded plugin for its state blob and stores it,
base64-encoded, next to the plugin's format, bundle path and class id. Reopening
the project loads the plugin again and replays the blob.

That blob is the plugin's own format — opaque to us, and version-specific to the
plugin. If a plugin changes its state format between versions, its presets may
not restore.

## Scanning rules

**CLAP** has no manifest, so listing a `.clap` library means loading it and
reading its descriptors. That happens on the scanner thread, inside
`catch_unwind`, and never on the audio thread.

**VST3** bundles ship `Contents/Resources/moduleinfo.json`, which lists every
class with its name, vendor, class id and sub-categories. Scanning reads that file
and never opens the library. Two consequences:

- A bundle without a manifest is skipped during scans, because describing it
  means loading it.
- A yabridge bundle is a Windows DLL wrapped in a Linux shim; asking the native
  loader about it starts a Wine process, and Wine aborting takes the scanner with
  it. This was found the hard way: the whole test binary died on a yabridge
  plugin. Manifest-only scanning removes the class of failure.

Bundles that ship a manifest but no library for this architecture are still
listed; the load fails with an error instead of crashing.

## Troubleshooting

- **A plugin is missing from the list** — it may be a bundle with no
  `moduleinfo.json`. Point `REPLICANTSYNTH_PLUGIN_PATH` at its folder and it will
  be offered on the next rescan.
- **The editor is blank** — the plugin probably needs its timers or run loop,
  which the host does drive; if it still happens, the plugin may be a
  Wayland-only build. Try **CLOSE** and reopen, or use **PIANO ROLL** and the
  parameter knobs.
- **No sound** — check that the plugin is an instrument (an effect on an empty pad
  has nothing to process), that the pad is playing, and that its notes exist in
  the piano roll. The status bar shows the block count once audio flows.
- **Crackling with one plugin** — some plugins need a larger block than 128
  frames. This is a known limit of the current bridge.
