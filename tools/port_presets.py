#!/usr/bin/env python3
"""Regenerate `crates/replicantsynth-core/src/model/wave_presets.rs`.

The 44 ReplicantWave factory sounds live in the ReplicantWave plug-in as a
Rust `preset!` list. Porting them by hand is error-prone, so this script reads
the upstream Kotlin transcription that ReplicantSynthAndroid already ships
(`model/WavePresets.kt`) and emits the Rust table used by the desktop engine.

Usage:

    python3 tools/port_presets.py \
        --source ../ReplicantSynthAndroid/app/src/main/java/com/danimanx/replicantsynth/model/WavePresets.kt \
        --out crates/replicantsynth-core/src/model/wave_presets.rs

The script keeps the parameter order the engine expects:
bank, position, unison, detune, spread, cutoff, resonance, drive, attack,
decay, sustain, release, sub, noise, level, filter_mode, chord (17 slots),
padded to the 32-float clip parameter block.
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path

ORDER = [
    "bank",
    "position",
    "unison",
    "detune",
    "spread",
    "cutoff",
    "resonance",
    "drive",
    "attack",
    "decay",
    "sustain",
    "release",
    "sub",
    "noise",
    "level",
    "filterMode",
]

HEADER = '''//! The 44 ReplicantWave factory sounds, generated from ReplicantWave 1.2.3
//! (`src/presets/factory.rs`) by `tools/port_presets.py`.
//!
//! The engine has one oscillator, so osc 2 / chorus / delay / reverb from the
//! desktop preset are represented by osc 1, sub, noise, drive and the per-pad
//! ReplicantFX insert instead of being silently dropped.

use super::clip::MidiNote;

#[derive(Clone)]
pub struct WavePreset {{
    pub name: &'static str,
    pub category: &'static str,
    pub base_note: i32,
    pub params: [f32; 32],
    pub notes: &'static [MidiNote],
}}

pub static WAVE_PRESETS: [WavePreset; {count}] = [
'''

FOOTER = '''];

pub fn wave_presets() -> &'static [WavePreset] {
    &WAVE_PRESETS
}
'''


def number(text: str) -> str:
    """Kotlin float literal -> Rust float literal."""
    cleaned = text.strip().rstrip("f")
    if "." not in cleaned and "e" not in cleaned.lower():
        cleaned += ".0"
    return cleaned


def parse(source: str) -> list[dict]:
    calls = re.findall(r"wavePreset\((.*?)\n    \),", source, re.S)
    presets: list[dict] = []
    for block in calls:
        def field(name: str) -> str | None:
            match = re.search(r"\b" + name + r"\s*=\s*([^,\n]+)", block)
            return match.group(1).strip() if match else None

        raw_name = field("name")
        notes_match = re.search(r"notes\s*=\s*intArrayOf\(([^)]*)\)", block)
        # Skip the helper function itself, which has no string literal name.
        if raw_name is None or not raw_name.startswith('"') or notes_match is None:
            continue

        values = {
            key: field(key)
            for key in ORDER
        }
        missing = [key for key, value in values.items() if value is None]
        if missing:
            raise SystemExit(f"preset {raw_name} is missing {missing}")

        numbers = [int(n) for n in re.findall(r"-?\d+", notes_match.group(1))]
        notes = []
        for index in range(0, len(numbers) - 3, 4):
            start, note, velocity, length = numbers[index:index + 4]
            notes.append((note, start, length, velocity))

        presets.append(
            {
                "name": raw_name.strip('"'),
                "category": (field("category") or '""').strip('"'),
                "base_note": field("baseNote"),
                "params": [values[key] for key in ORDER],
                "notes": notes,
            }
        )
    return presets


def render(presets: list[dict]) -> str:
    out = [HEADER.format(count=len(presets))]
    for preset in presets:
        params = [number(value) for value in preset["params"]] + ["0.0"]
        params += ["0.0"] * (32 - len(params))
        out.append("    WavePreset {")
        out.append(f'        name: "{preset["name"]}",')
        out.append(f'        category: "{preset["category"]}",')
        out.append(f'        base_note: {preset["base_note"]},')
        out.append(f'        params: [{", ".join(params)}],')
        if preset["notes"]:
            notes = ", ".join(
                f"MidiNote {{ note: {note}, start: {start}, length: {length}, velocity: {velocity} }}"
                for note, start, length, velocity in preset["notes"]
            )
            out.append(f"        notes: &[{notes}],")
        else:
            out.append("        notes: &[],")
        out.append("    },")
    out.append(FOOTER)
    return "\n".join(out)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()

    presets = parse(args.source.read_text(encoding="utf-8"))
    if not presets:
        raise SystemExit(f"no presets found in {args.source}")
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(render(presets), encoding="utf-8")
    print(f"wrote {len(presets)} presets to {args.out}")


if __name__ == "__main__":
    main()
