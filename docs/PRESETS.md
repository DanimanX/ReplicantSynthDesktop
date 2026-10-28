# Presets

The preset banks are ports of the factory content that ships with the four
Replicant instruments. Every preset only replaces a clip's sound parameters and
pattern; the channel strip, the ReplicantFX insert and its six macros are left
untouched, so browsing never destroys a mix.

## Replicant303 — 512 acid lines

`AcidPresets` builds eight banks of 64 slots. Slot 0 of each bank is the named
factory line (`Amber Line`, `Sub Level`, `Neon Run`, `Acid Rain`, `Blade Pulse`,
`Rooftop Chase`, `Off World`, `Last Replicant`); slots 1–63 are generated
deterministically by a SplitMix32 stream seeded from `(bank, slot)` and the
constant `0x52443303`.

Each generated line combines:

- a **scale** per bank (minor pentatonic, natural minor, Dorian, Phrygian,
  blues, Mixolydian, harmonic minor, chromatic),
- a **length** drawn from `{16, 16, 16, 16, 16, 12, 8, 7}` steps,
- a **gate density** of 62–94 %, accent probability 16–38 % and slide
  probability 10–32 %,
- a **character** chosen by `(bank + slot) % 8`: classic acid, sub pressure,
  bright/open, screamer, germanium fuzz, wavefolder, clean/dry or industrial.

Because the generator is deterministic, the bank is identical on every machine
and in every session — the same guarantee the desktop Replicant303 makes.

In the app: **SOUND → BROWSE** with the `303` bank selected. The list shows the
bank letter and slot (`A01 · Amber Line`) and the pattern length.

## Replicant909 — 8 classic patterns

| Name | Category | Character |
| --- | --- | --- |
| Classic House | House | four-on-the-floor, open hats on the off-beats |
| Detroit Techno | Techno | driving kick, rim shots, bright ride |
| Acid Jack | Acid | busy hats, sparse snare, rim accents |
| Hard Groove | Techno | dense kick, aggressive ride |
| Electro Funk | Electro | syncopated kick, claps, off-beat hats |
| Industrial Rave | Industrial | crash-heavy, relentless kick |
| Minimal Techno | Minimal | kick, rim, hats only |
| Synthwave 909 | Synthwave | kick, snare, closed hats, toms |

Applying a drum preset also sets the kit macro values (accent, level, cutoff,
resonance) that the pattern was voiced for.

## ReplicantWave — 44 sounds

The 44 factory sounds come from ReplicantWave 1.2.3 `src/presets/factory.rs`,
converted by `tools/port_presets.py` into `model/wave_presets.rs`. Categories:
Bass, Pad, Lead, Acid, Arp, Stab, Drone, Combat, Choir, Sub and more.

The clip engine has one oscillator, so the desktop plug-in's second oscillator,
chorus, delay and reverb are represented by osc 1, sub, noise, drive and the
per-pad ReplicantFX insert rather than being silently dropped. The notes in each
preset are carried over as MIDI notes, so loading a preset fills the piano roll
with the phrase the sound was designed for.

In the app: **SOUND → BROWSE** with the `WAVE` bank selected.

## ReplicantFX — 48 slots

The FX catalog (`dsp::fx::catalog`) mirrors the SP-404MKII v5.52 firmware table:
name, three-character LED code, six parameter labels (CTRL 1–3 / SUB 4–6) and
the factory default for each. Applying a slot copies those six defaults into the
pad's macro block and enables the insert; `Bypass` (slot 0) disables it.

The inspector relabels the six knobs as you change slot, so `F+D` shows
CUTOFF / RESONANCE / DRIVE / FLT TYPE / LOW FREQ / LOW GAIN while `REV` shows
TIME / LEVEL / PRE DELAY / TYPE / LOW CUT / HIGH CUT.

## Demo project

`demo_project()` builds **NEON GRID**: 126 BPM, one-bar launch quantization and
six scenes.

| Row | Pads |
| --- | --- |
| 1 | ACID BASS (303 + F+D) · 909 KIT (909 + ISO) · NEON PAD (Wave + JNO) |
| 2 | ACID LEAD (303 + WAH) · 909 RESO (909 + RES) · ACID ARP (Wave + SYD) |
| 3 | NEON VOX (sample + SAT) · FILTER BASS (303 + SFL) · VINYL 909 (909 + 404) |
| 4 | FM LEAD (Wave + ZNZ) · DIST ACID (303 + DST) · DELAY KIT (909 + TCD) |
| 5 | VOX PAD (Wave + SXR) · ZAN-ZOU (303 + ZNZ) |
| 6 | LOOP ACID (303 + DJL) · FULL KIT (909 + CHR) · SUB LEAD (Wave + CHR) · VOX CLOUD (sample + CLD) |

The two sample pads load the bundled 3-second vocal stab
(`assets/demo/neon_stab.wav`) copied into the data directory on first launch. If
the copy fails, the pads fall back to `NO SAMPLE` and stay silent rather than
playing garbage.
