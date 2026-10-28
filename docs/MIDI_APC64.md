# MIDI and the APC64

ReplicantSynth Desktop speaks plain MIDI 1.0 over `midir`, and treats the Akai
APC64 as a first-class controller. Every other controller works through the same
parser; the APC64 profile just adds cable routing, LEDs and the OLED.

## Ports

The APC64 exposes four USB MIDI cables, which the operating system presents as
separate ports:

| Cable | Role | Used for |
| --- | --- | --- |
| DAW | clip grid, transport, scene buttons, encoder, touch strips | launching and transport |
| Notes | the unit's Note / Chord keyboard and its Step Sequencer | auditioning and step input |
| MIDI | 5-pin MIDI I/O | pass-through |
| Custom | the unit's own custom-mode data | ignored |

`midi::port_role` resolves a port by name (`DAW`, `NOTE`, `CUSTOM`, `MIDI` in the
name, case-insensitive) and falls back to the port index (0 DAW, 1 Notes,
2 MIDI, 3 Custom) when the name is generic. Each port gets its own `MidiInput`
because midir consumes the handle on a failed connect; that way one busy cable
does not close the others.

**MIDI → CONNECT APC64** opens every port whose name contains `APC64`.
**CONNECT ALL** opens everything, which is what you want for a plain keyboard.

## Pad mode

After the handshake the unit reports its pad mode:

```
F0 47 00 53 19 00 01 <mode> F7
```

On firmware **1.2.1.6** the validated map is:

| Id | Mode | Pads are |
| --- | --- | --- |
| 0 | Session | clips (host-drawn) |
| 1 | Overview | clips (host-drawn) |
| 2 | Note | keys |
| 3 | Chord | keys |
| 4 | Chord settings | the unit's own data |
| 5 | Drum | the unit's own data |
| 6 | Step | the unit's Step Sequencer |
| 7 | Step settings | the unit's own data |
| 8 | Project | the unit's own data |
| 9 | Custom | the unit's own data |

The app routes by cable and by mode:

- **Notes cable, Step mode** → pad presses toggle steps on the selected 303 or
  909 clip instead of playing keys.
- **Notes cable, other modes** → key presses audition the selected clip's engine.
- **DAW cable, Session/Overview** → pads launch and stop clips.
- **DAW cable, Note/Chord** → pads audition keys, never launch clips, so a merged
  USB implementation cannot fire a clip from the keyboard surface.
- **Transport, scene and cursor notes** are recognised on any cable.

## Handshake

Sent on connect, and again from **MIDI → HANDSHAKE**:

```
F0 47 00 53 1B 00 01 00 F7   track type
F0 47 00 53 19 00 01 00 F7   generic/DAW mode
F0 7E 7F 06 01 F7            identity request
F0 47 00 53 1C 00 01 01 F7   take display ownership
F0 47 00 53 19 00 01 00 F7   DAW mode again
F0 47 00 53 10 00 ... F7     OLED "REPLICANT"
F0 47 00 53 10 01 ... F7     OLED "SYNTH"
F0 47 00 53 10 02 ... F7     OLED "64 PAD"
F0 47 00 53 10 03 ... F7     OLED "CLIP GRID"
```

The identity reply is parsed:

```
F0 7E 00 06 02 47 53 00 19 <fw0> <fw1> <fw2> <fw3> ... F7
```

and printed as `major.minor.patch.build` in the MIDI sheet.

## Mapping

| Control | Message | Action |
| --- | --- | --- |
| 64 pads | Notes 0–63, note 0 = bottom-left | Launch / stop / select clip |
| Scene buttons | Notes 112–119 | Launch row |
| Play / Stop | Notes 91 / 93 | Transport start / stop |
| Record | Note 92 | Toggle master capture |
| Clip Stop | Note 111 | Stop all clips |
| Mute / Solo | Notes 109 / 110 | Toggle the selected pad |
| Cursor up / down / left / right | Notes 94 / 95 / 96 / 97 | Move the pad selection |
| Encoder | CC 90 (two's complement) | Nudge BPM by 0.5 per click |
| Encoder press | Note 90 | Reserved for the focused field |
| Touch strips | Pitch-bend channels 1–8 | Strip 1 master gain, strip 8 pad gain |
| Strip touch | Notes 82–89 | Reserved for per-strip focus |

### Switches

Two switches in the MIDI sheet cover the layouts that differ between firmwares
and DAW scripts:

- **BASE 0 / BASE 36** — stock Ableton DAW mode starts the pads at note 0; the
  Akai custom project layout and FreeAPC64 usually start at 36.
- **FLIP PAD ROWS** — notes ascend from the bottom-left pad on the hardware, while
  the on-screen grid is top-left row-major. Flipping makes pad 1 the top-left
  cell, which is what the screen shows.

## LED feedback

Pad state is pushed at roughly 10 Hz, and only when the state key changes:

| Clip state | Colour | Brightness |
| --- | --- | --- |
| empty, unselected | grey | half |
| empty, selected | white | half |
| queued start | yellow | blink |
| playing 303 | amber | full |
| playing Wave | blue | full |
| playing 909 | yellow | full |
| playing sample | green | full |
| queued stop | red | blink |
| stopped, filled | amber | full if selected, else half |

Scene buttons are green while any pad in their row is playing, grey otherwise.

Touch-strip LEDs use the pitch-bend position on the strip's own channel plus CC
104–111 (mode) and CC 112–119 (colour), the same messages Bitwig's APC64
extension sends.

The OLED shows `REPLICANT`, the BPM, the quantize setting and the selected pad.

## Troubleshooting

- **Pads launch nothing** — check that the unit is in Session or Overview mode;
  in Note/Chord the pad surface is a keyboard by design.
- **Wrong pads fire** — flip **BASE 0 / BASE 36** or **FLIP PAD ROWS**.
- **No LED feedback** — the app needs an output port. If only an input opened,
  the MIDI sheet shows the error; close the DAW that is holding the port.
- **Linux permissions** — add the user to the `audio` group and, on systems with
  raw ALSA MIDI, install a udev rule for the APC64.
