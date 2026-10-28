//! The 48-slot SP-404MKII MFX catalog: names, LED codes, the six dynamic
//! parameter labels and the factory defaults from the v5.52 firmware table.

pub const FX_COUNT: usize = 48;

#[derive(Clone, Copy, Debug)]
pub struct FxInfo {
    pub name: &'static str,
    pub code: &'static str,
    pub labels: [&'static str; 6],
    pub defaults: [f32; 6],
}

pub static FX_TABLE: [FxInfo; FX_COUNT] = [
    FxInfo {
        name: "Bypass",
        code: "BYP",
        labels: ["GAIN", "PAN", "LOW TRIM", "PHASE", "HIGH TRIM", "BALANCE"],
        defaults: [0.5, 0.5, 0.5, 0.0, 0.5, 1.0],
    },
    FxInfo {
        name: "Filter+Drive",
        code: "F+D",
        labels: [
            "CUTOFF",
            "RESONANCE",
            "DRIVE",
            "FLT TYPE",
            "LOW FREQ",
            "LOW GAIN",
        ],
        defaults: [0.62, 0.35, 0.3, 0.0, 0.35, 0.55],
    },
    FxInfo {
        name: "TimeCtrlDly",
        code: "TCD",
        labels: [
            "TIME", "FEEDBACK", "LEVEL", "HIGH CUT", "LOW DAMP", "BALANCE",
        ],
        defaults: [0.42, 0.52, 0.65, 0.68, 0.3, 0.55],
    },
    FxInfo {
        name: "Isolator",
        code: "ISO",
        labels: ["LOW", "MID", "HIGH", "KILL LOW", "KILL MID", "KILL HIGH"],
        defaults: [0.55, 0.5, 0.55, 0.0, 0.0, 0.0],
    },
    FxInfo {
        name: "Crusher",
        code: "CRS",
        labels: [
            "FILTER",
            "RATE",
            "BALANCE",
            "BIT DEPTH",
            "RESONANCE",
            "DRIVE",
        ],
        defaults: [0.58, 0.45, 0.75, 0.45, 0.3, 0.35],
    },
    FxInfo {
        name: "Phaser",
        code: "PHS",
        labels: ["RATE", "DEPTH", "RESONANCE", "MANUAL", "TYPE", "BALANCE"],
        defaults: [0.32, 0.68, 0.45, 0.5, 0.33, 0.7],
    },
    FxInfo {
        name: "Flanger",
        code: "FLG",
        labels: [
            "RATE",
            "DEPTH",
            "RESONANCE",
            "MANUAL",
            "LOW CUT F",
            "BALANCE",
        ],
        defaults: [0.28, 0.65, 0.58, 0.45, 0.25, 0.65],
    },
    FxInfo {
        name: "Overdrive",
        code: "ODV",
        labels: ["DRIVE", "TONE", "BALANCE", "LEVEL", "AMP TYPE", "LOW CUT"],
        defaults: [0.55, 0.58, 0.8, 0.65, 0.0, 0.2],
    },
    FxInfo {
        name: "Distortion",
        code: "DST",
        labels: ["DRIVE", "TONE", "BALANCE", "LEVEL", "MODE", "LOW BOOST"],
        defaults: [0.68, 0.52, 0.82, 0.62, 0.67, 0.55],
    },
    FxInfo {
        name: "Tape Echo",
        code: "TPE",
        labels: [
            "TIME",
            "FEEDBACK",
            "LEVEL",
            "MODE",
            "TAPE DIST",
            "W/F DEPTH",
        ],
        defaults: [0.46, 0.58, 0.62, 0.5, 0.35, 0.4],
    },
    FxInfo {
        name: "Reverb",
        code: "REV",
        labels: ["TIME", "LEVEL", "PRE DELAY", "TYPE", "LOW CUT", "HIGH CUT"],
        defaults: [0.6, 0.55, 0.25, 0.5, 0.25, 0.72],
    },
    FxInfo {
        name: "WrmSaturator",
        code: "SAT",
        labels: [
            "DRIVE", "EQ LOW", "EQ HIGH", "LOW FREQ", "HI FREQ", "BALANCE",
        ],
        defaults: [0.52, 0.6, 0.56, 0.35, 0.68, 0.85],
    },
    FxInfo {
        name: "Resonator",
        code: "RES",
        labels: ["ROOT", "BRIGHT", "FEEDBACK", "CHORD", "ENV MOD", "PANNING"],
        defaults: [0.35, 0.65, 0.68, 0.45, 0.4, 0.6],
    },
    FxInfo {
        name: "DJFX Looper",
        code: "DJL",
        labels: ["LENGTH", "SPEED", "LOOP SW", "REVERSE", "TONE", "LEVEL"],
        defaults: [0.55, 0.75, 0.0, 0.0, 0.65, 0.85],
    },
    FxInfo {
        name: "Chorus",
        code: "CHR",
        labels: ["RATE", "DEPTH", "BALANCE", "EQ LOW", "EQ HIGH", "LEVEL"],
        defaults: [0.35, 0.6, 0.65, 0.55, 0.6, 0.75],
    },
    FxInfo {
        name: "JUNO Chorus",
        code: "JNO",
        labels: ["MODE", "NOISE", "BALANCE", "RATE", "DEPTH", "LEVEL"],
        defaults: [0.5, 0.22, 0.75, 0.4, 0.68, 0.8],
    },
    FxInfo {
        name: "Tremolo/Pan",
        code: "TRM",
        labels: ["RATE", "DEPTH", "TYPE", "WAVE", "EQ LOW", "EQ HIGH"],
        defaults: [0.48, 0.72, 0.5, 0.35, 0.5, 0.55],
    },
    FxInfo {
        name: "Wah",
        code: "WAH",
        labels: ["MANUAL", "PEAK", "RATE", "FLT TYPE", "DEPTH", "SENS"],
        defaults: [0.45, 0.7, 0.38, 1.0, 0.65, 0.6],
    },
    FxInfo {
        name: "Sync Delay",
        code: "SYD",
        labels: [
            "TIME",
            "FEEDBACK",
            "LEVEL",
            "HIGH CUT",
            "HIGH DAMP",
            "LOW DAMP",
        ],
        defaults: [0.48, 0.55, 0.6, 0.72, 0.45, 0.3],
    },
    FxInfo {
        name: "Equalizer",
        code: "EQL",
        labels: [
            "LOW GAIN",
            "MID GAIN",
            "HIGH GAIN",
            "LOW FREQ",
            "MID FREQ",
            "MID Q",
        ],
        defaults: [0.58, 0.52, 0.58, 0.32, 0.5, 0.45],
    },
    FxInfo {
        name: "Lo-fi",
        code: "LFI",
        labels: [
            "PRE FILT",
            "LOFI TYPE",
            "TONE",
            "CUTOFF",
            "POST FILT",
            "BALANCE",
        ],
        defaults: [0.4, 0.45, 0.52, 0.58, 0.45, 0.8],
    },
    FxInfo {
        name: "Ring Mod",
        code: "RNG",
        labels: [
            "FREQUENCY",
            "SENS",
            "BALANCE",
            "POLARITY",
            "EQ LOW",
            "EQ HIGH",
        ],
        defaults: [0.42, 0.55, 0.65, 0.0, 0.5, 0.55],
    },
    FxInfo {
        name: "Chromatic PS",
        code: "CPS",
        labels: [
            "PITCH1",
            "PITCH2",
            "BALANCE",
            "FINE1",
            "FINE2",
            "PAN SPREAD",
        ],
        defaults: [0.65, 0.35, 0.6, 0.5, 0.5, 0.7],
    },
    FxInfo {
        name: "Gt Amp Sim",
        code: "AMP",
        labels: ["AMP TYPE", "DRIVE", "LEVEL", "BASS", "MIDDLE", "TREBLE"],
        defaults: [0.4, 0.58, 0.65, 0.55, 0.52, 0.58],
    },
    FxInfo {
        name: "Stopper",
        code: "STP",
        labels: ["RATE", "STOP TRIG", "DEPTH", "FILTER", "RESONANCE", "LEVEL"],
        defaults: [0.52, 0.0, 0.8, 0.68, 0.35, 0.85],
    },
    FxInfo {
        name: "Downer",
        code: "DWN",
        labels: ["DEPTH", "RATE", "FILTER", "PITCH", "RESONANCE", "LEVEL"],
        defaults: [0.62, 0.45, 0.58, 0.35, 0.48, 0.8],
    },
    FxInfo {
        name: "Slicer",
        code: "SLC",
        labels: ["PATTERN", "SPEED", "DEPTH", "ATTACK", "SHUFFLE", "MODE"],
        defaults: [0.35, 0.55, 0.82, 0.25, 0.3, 0.0],
    },
    FxInfo {
        name: "Auto Pitch",
        code: "ATP",
        labels: ["PITCH", "FORMANT", "BALANCE", "AT PITCH", "KEY", "ROBOT"],
        defaults: [0.5, 0.55, 0.75, 0.7, 0.0, 0.0],
    },
    FxInfo {
        name: "Compressor",
        code: "CMP",
        labels: ["SUSTAIN", "ATTACK", "LEVEL", "RATIO", "TONE", "BALANCE"],
        defaults: [0.65, 0.35, 0.68, 0.6, 0.55, 0.9],
    },
    FxInfo {
        name: "303 BusComp",
        code: "C02",
        labels: ["SUSTAIN", "ATTACK", "LEVEL", "RATIO", "TONE", "BALANCE"],
        defaults: [0.78, 0.25, 0.7, 0.82, 0.52, 0.95],
    },
    FxInfo {
        name: "Scatter",
        code: "SCT",
        labels: ["SCATTER", "DEPTH", "SPEED", "GATE", "ATTACK", "BALANCE"],
        defaults: [0.3, 0.72, 0.55, 0.5, 0.2, 0.85],
    },
    FxInfo {
        name: "404 VinylSim",
        code: "404",
        labels: [
            "FREQUENCY",
            "NOISE",
            "WOW FLUT",
            "DIST",
            "DISC TYPE",
            "BALANCE",
        ],
        defaults: [0.58, 0.42, 0.48, 0.35, 0.0, 0.85],
    },
    FxInfo {
        name: "303 VinylSim",
        code: "303",
        labels: ["COMP", "NOISE", "WOW FLUT", "AGE", "TONE", "BALANCE"],
        defaults: [0.68, 0.45, 0.52, 0.5, 0.48, 0.9],
    },
    FxInfo {
        name: "Zan-Zou",
        code: "ZNZ",
        labels: ["TIME", "FEEDBACK", "HF DAMP", "MODE", "SPREAD", "LEVEL"],
        defaults: [0.45, 0.6, 0.55, 0.5, 0.8, 0.68],
    },
    FxInfo {
        name: "Super Filter",
        code: "SFL",
        labels: [
            "CUTOFF",
            "RESONANCE",
            "FLT TYPE",
            "FLT SLOPE",
            "RATE",
            "DEPTH",
        ],
        defaults: [0.55, 0.52, 0.0, 0.5, 0.45, 0.6],
    },
    FxInfo {
        name: "Hyper-Reso",
        code: "HYP",
        labels: [
            "NOTE",
            "SPREAD",
            "FEEDBACK",
            "CHARACTER",
            "ENV MOD",
            "LEVEL",
        ],
        defaults: [0.42, 0.65, 0.72, 0.58, 0.45, 0.7],
    },
    FxInfo {
        name: "Ko-Da-Ma",
        code: "KDM",
        labels: ["TIME", "FEEDBACK", "SEND", "MODE", "HIGH CUT", "MOD DEPTH"],
        defaults: [0.48, 0.62, 0.75, 0.5, 0.65, 0.45],
    },
    FxInfo {
        name: "Ha-Dou",
        code: "HDO",
        labels: [
            "TIME",
            "LEVEL",
            "MOD DEPTH",
            "PRE DELAY",
            "LOW CUT",
            "DENSITY",
        ],
        defaults: [0.75, 0.62, 0.58, 0.35, 0.3, 0.8],
    },
    FxInfo {
        name: "To-Gu-Ro",
        code: "TGR",
        labels: ["DEPTH", "RATE", "RESONANCE", "FLT MOD", "AMP MOD", "HOLD"],
        defaults: [0.68, 0.48, 0.55, 0.65, 0.5, 0.0],
    },
    FxInfo {
        name: "Cassette Sim",
        code: "CAS",
        labels: ["TONE", "HISS", "WOW FLUT", "AGE", "DRIVE", "CATCH"],
        defaults: [0.54, 0.35, 0.48, 0.45, 0.52, 0.25],
    },
    FxInfo {
        name: "SBF",
        code: "SBF",
        labels: ["INTERVAL", "WIDTH", "BALANCE", "TYPE", "GAIN", "LOW CUT"],
        defaults: [0.52, 0.48, 0.75, 0.35, 0.55, 0.25],
    },
    FxInfo {
        name: "SX Delay",
        code: "SXD",
        labels: [
            "TIME",
            "FEEDBACK",
            "BALANCE",
            "FBK ROUTE",
            "HF DAMP",
            "MOD DEPTH",
        ],
        defaults: [0.44, 0.56, 0.58, 0.5, 0.52, 0.35],
    },
    FxInfo {
        name: "Vocoder",
        code: "VOC",
        labels: ["NOTE", "FORMANT", "TONE", "CHORD", "COLOR", "BALANCE"],
        defaults: [0.4, 0.55, 0.62, 0.45, 0.58, 0.85],
    },
    FxInfo {
        name: "Harmony",
        code: "HRM",
        labels: ["PITCH", "HARMONY", "BALANCE", "FORMANT", "KEY", "AT PITCH"],
        defaults: [0.5, 0.6, 0.65, 0.52, 0.25, 0.6],
    },
    FxInfo {
        name: "SX Reverb",
        code: "SXR",
        labels: ["TIME", "TONE", "BALANCE", "TYPE", "PRE DELAY", "LEVEL"],
        defaults: [0.58, 0.6, 0.55, 0.4, 0.25, 0.75],
    },
    FxInfo {
        name: "Sub Sonic",
        code: "SUB",
        labels: ["FREQ", "SUB LEVEL", "BALANCE", "DRIVE", "TONE", "DIRECT"],
        defaults: [0.38, 0.68, 0.75, 0.45, 0.42, 0.8],
    },
    FxInfo {
        name: "Back Spin",
        code: "BSP",
        labels: ["LENGTH", "SPEED", "BACK SW", "CURVE", "TONE", "LEVEL"],
        defaults: [0.55, 0.72, 0.0, 0.5, 0.65, 0.85],
    },
    FxInfo {
        name: "Cloud Delay",
        code: "CLD",
        labels: ["PITCH", "WINDOW", "FEEDBACK", "CLOUDY", "LOFI", "BALANCE"],
        defaults: [0.75, 0.62, 0.65, 0.78, 0.3, 0.62],
    },
];

/// Look up a slot, clamping out-of-range ids to the Bypass slot.
pub fn fx_info(id: usize) -> &'static FxInfo {
    &FX_TABLE[id.min(FX_COUNT - 1)]
}

/// The 48-slot catalog as a slice, for browsers and tests.
pub fn fx_catalog() -> &'static [FxInfo] {
    &FX_TABLE
}
