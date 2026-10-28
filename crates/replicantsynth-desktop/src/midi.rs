//! MIDI input/output, including the Akai APC64 profile.
//!
//! The APC64 exposes four USB MIDI cables (DAW, Notes, MIDI, Custom). The hub
//! opens every port whose name matches the unit, parses the stream, publishes
//! events to the UI thread and can send the Akai handshake, pad LEDs, touch
//! strip LEDs and OLED labels back.

use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use std::sync::mpsc::{channel, Receiver, Sender};

pub const PAD_COUNT: usize = 64;
pub const PAD_BASE_DAW: i32 = 0;
pub const PAD_BASE_CUSTOM: i32 = 36;

pub const LED_FULL: u8 = 6;
pub const LED_HALF: u8 = 0;
pub const LED_BLINK: u8 = 14;

pub const COLOR_OFF: u8 = 0;
pub const COLOR_GREY: u8 = 1;
pub const COLOR_WHITE: u8 = 3;
pub const COLOR_RED: u8 = 5;
pub const COLOR_AMBER: u8 = 9;
pub const COLOR_YELLOW: u8 = 13;
pub const COLOR_GREEN: u8 = 21;
pub const COLOR_BLUE: u8 = 45;

pub const NOTE_PLAY: i32 = 91;
pub const NOTE_RECORD: i32 = 92;
pub const NOTE_STOP: i32 = 93;
pub const NOTE_ENCODER_BUTTON: i32 = 90;
pub const NOTE_ARROW_UP: i32 = 94;
pub const NOTE_ARROW_DOWN: i32 = 95;
pub const NOTE_ARROW_LEFT: i32 = 96;
pub const NOTE_ARROW_RIGHT: i32 = 97;
pub const NOTE_CLIP_STOP: i32 = 111;
pub const NOTE_MUTE: i32 = 109;
pub const NOTE_SOLO: i32 = 110;
pub const CC_ENCODER: i32 = 90;
pub const SCENE_FIRST: i32 = 112;
pub const SCENE_LAST: i32 = 119;
pub const STRIP_TOUCH_FIRST: i32 = 82;
pub const STRIP_TOUCH_LAST: i32 = 89;
pub const STRIP_COUNT: usize = 8;
pub const CC_STRIP_MODE_FIRST: u8 = 104;
pub const CC_STRIP_COLOR_FIRST: u8 = 112;

pub const VALIDATED_FIRMWARE: &str = "1.2.1.6";

/// The unit's pad-mode report, validated against firmware 1.2.1.6.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ApcPadMode {
    Session,
    Overview,
    Note,
    Chord,
    ChordSettings,
    Drum,
    Step,
    StepSettings,
    Project,
    Custom,
    #[default]
    Unknown,
}

impl ApcPadMode {
    pub fn from_id(id: i32) -> Self {
        match id {
            0 => Self::Session,
            1 => Self::Overview,
            2 => Self::Note,
            3 => Self::Chord,
            4 => Self::ChordSettings,
            5 => Self::Drum,
            6 => Self::Step,
            7 => Self::StepSettings,
            8 => Self::Project,
            9 => Self::Custom,
            _ => Self::Unknown,
        }
    }

    pub fn id(self) -> i32 {
        match self {
            Self::Session => 0,
            Self::Overview => 1,
            Self::Note => 2,
            Self::Chord => 3,
            Self::ChordSettings => 4,
            Self::Drum => 5,
            Self::Step => 6,
            Self::StepSettings => 7,
            Self::Project => 8,
            Self::Custom => 9,
            Self::Unknown => -1,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Session => "SESSION",
            Self::Overview => "OVERVIEW",
            Self::Note => "NOTE",
            Self::Chord => "CHORD",
            Self::ChordSettings => "CHORD CFG",
            Self::Drum => "DRUM",
            Self::Step => "STEP",
            Self::StepSettings => "STEP CFG",
            Self::Project => "PROJECT",
            Self::Custom => "CUSTOM",
            Self::Unknown => "UNKNOWN",
        }
    }

    /// Session and Overview are host-drawn: the 64 pads are clip slots.
    pub fn host_owns_pads(self) -> bool {
        matches!(self, Self::Session | Self::Overview)
    }

    /// The unit's Note and Chord surfaces send real key presses.
    pub fn key_surface(self) -> bool {
        matches!(self, Self::Note | Self::Chord)
    }
}

/// Which USB MIDI cable an event arrived on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortRole {
    Daw,
    Notes,
    Custom,
    Midi,
    Unknown,
}

pub fn port_role(name: &str, index: usize) -> PortRole {
    let upper = name.to_uppercase();
    if upper.contains("DAW") {
        PortRole::Daw
    } else if upper.contains("NOTE") {
        PortRole::Notes
    } else if upper.contains("CUSTOM") {
        PortRole::Custom
    } else if upper.contains("MIDI") {
        PortRole::Midi
    } else {
        match index {
            0 => PortRole::Daw,
            1 => PortRole::Notes,
            2 => PortRole::Midi,
            3 => PortRole::Custom,
            _ => PortRole::Unknown,
        }
    }
}

/// note 0 is the bottom-left pad; the UI grid is top-left row-major.
pub fn pad_for_note(note: i32, base_note: i32, flip_rows: bool) -> Option<usize> {
    let offset = note - base_note;
    if !(0..PAD_COUNT as i32).contains(&offset) {
        return None;
    }
    let from_bottom = (offset / 8) as usize;
    let column = (offset % 8) as usize;
    let row = if flip_rows {
        7 - from_bottom
    } else {
        from_bottom
    };
    Some(row * 8 + column)
}

pub fn note_for_pad(pad: usize, base_note: i32, flip_rows: bool) -> i32 {
    let row = pad / 8;
    let column = pad % 8;
    let from_bottom = if flip_rows { 7 - row } else { row };
    base_note + (from_bottom * 8 + column) as i32
}

pub fn scene_for_note(note: i32) -> Option<usize> {
    if (SCENE_FIRST..=SCENE_LAST).contains(&note) {
        Some((note - SCENE_FIRST) as usize)
    } else {
        None
    }
}

pub fn strip_touch_for_note(note: i32) -> Option<usize> {
    if (STRIP_TOUCH_FIRST..=STRIP_TOUCH_LAST).contains(&note) {
        Some((note - STRIP_TOUCH_FIRST) as usize)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrow {
    Up,
    Down,
    Left,
    Right,
}

pub fn arrow_for_note(note: i32) -> Option<Arrow> {
    match note {
        NOTE_ARROW_UP => Some(Arrow::Up),
        NOTE_ARROW_DOWN => Some(Arrow::Down),
        NOTE_ARROW_LEFT => Some(Arrow::Left),
        NOTE_ARROW_RIGHT => Some(Arrow::Right),
        _ => None,
    }
}

/// Parse the Akai SysEx envelope, returning the message id and payload.
pub fn parse_akai_message(bytes: &[u8]) -> Option<(u8, &[u8])> {
    if bytes.len() < 9 {
        return None;
    }
    if bytes[0] != 0xF0 || bytes[1] != 0x47 || bytes[2] != 0x00 || bytes[3] != 0x53 {
        return None;
    }
    if bytes[bytes.len() - 1] != 0xF7 {
        return None;
    }
    let id = bytes[4] & 0x7F;
    let length = ((bytes[5] as usize & 0x7F) << 7) | (bytes[6] as usize & 0x7F);
    let start = 7;
    if start + length + 1 != bytes.len() {
        return None;
    }
    Some((id, &bytes[start..start + length]))
}

pub fn pad_mode_from_sysex(bytes: &[u8]) -> Option<ApcPadMode> {
    let (id, payload) = parse_akai_message(bytes)?;
    if id != 0x19 || payload.len() != 1 {
        return None;
    }
    let mode = ApcPadMode::from_id(payload[0] as i32 & 0x7F);
    if mode == ApcPadMode::Unknown {
        None
    } else {
        Some(mode)
    }
}

/// Universal identity reply: `F0 7E 00 06 02 47 53 00 19 <fw0 fw1 fw2 fw3> ...`.
pub fn firmware_from_identity(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 17 {
        return None;
    }
    const PREFIX: [u8; 9] = [0xF0, 0x7E, 0x00, 0x06, 0x02, 0x47, 0x53, 0x00, 0x19];
    if bytes[..9] != PREFIX {
        return None;
    }
    let fw: Vec<u8> = (0..4).map(|i| bytes[9 + i]).collect();
    if fw.iter().any(|v| *v > 99) {
        return None;
    }
    Some(format!("{}.{}.{}.{}", fw[0], fw[1], fw[2], fw[3]))
}

pub fn firmware_supported(version: Option<&str>) -> bool {
    let Some(version) = version else {
        return false;
    };
    let parts: Vec<i32> = version
        .split('.')
        .filter_map(|part| part.parse::<i32>().ok())
        .collect();
    if parts.len() < 4 {
        return false;
    }
    const MIN: [i32; 4] = [1, 2, 1, 6];
    for i in 0..4 {
        if parts[i] != MIN[i] {
            return parts[i] > MIN[i];
        }
    }
    true
}

pub fn sysex(payload: &[i32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 2);
    out.push(0xF0);
    for value in payload {
        out.push((*value & 0x7F) as u8);
    }
    out.push(0xF7);
    out
}

/// `F0 47 00 53 <id> <lenMsb> <lenLsb> <payload...> F7`
pub fn akai_message(message_id: i32, values: &[i32]) -> Vec<u8> {
    let len = values.len() as i32;
    let mut payload = vec![0x47, 0x00, 0x53, message_id, (len >> 7) & 0x7F, len & 0x7F];
    payload.extend_from_slice(values);
    sysex(&payload)
}

pub fn mode_message(mode: i32) -> Vec<u8> {
    akai_message(0x19, &[mode & 0x7F])
}

pub fn track_type_message(kind: i32) -> Vec<u8> {
    akai_message(0x1B, &[kind & 0x01])
}

pub fn display_owner_message(owner: i32) -> Vec<u8> {
    akai_message(0x1C, &[owner & 0x01])
}

pub fn oled_line(line_index: usize, text: &str) -> Vec<u8> {
    let index = line_index.min(7);
    let offset = if index >= 4 { 0x1A } else { 0x10 };
    let line = index & 0x03;
    let mut payload = vec![line as i32];
    for c in text.chars().take(8) {
        payload.push(c as i32 & 0x7F);
    }
    payload.push(0);
    akai_message(offset, &payload)
}

pub fn identity_request() -> Vec<u8> {
    vec![0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7]
}

/// The sequence Ableton and Bitwig send to unlock the 64-pad surface.
pub fn handshake() -> Vec<Vec<u8>> {
    let session = ApcPadMode::Session.id();
    vec![
        track_type_message(0),
        mode_message(session),
        identity_request(),
        display_owner_message(1),
        mode_message(session),
        oled_line(0, "REPLICANT"),
        oled_line(1, "SYNTH"),
        oled_line(2, "64 PAD"),
        oled_line(3, "CLIP GRID"),
    ]
}

pub fn led(note: i32, color: u8, channel: u8) -> [u8; 3] {
    [0x90 | (channel & 0x0F), (note & 0x7F) as u8, color & 0x7F]
}

pub fn cc(controller: u8, value: u8) -> [u8; 3] {
    [0xB0, controller & 0x7F, value & 0x7F]
}

pub fn pitch_bend(channel: u8, value: i32) -> [u8; 3] {
    let v = value.clamp(0, 16383);
    [
        0xE0 | (channel & 0x0F),
        (v & 0x7F) as u8,
        ((v >> 7) & 0x7F) as u8,
    ]
}

/// LED colour and brightness for a pad, mirroring the clip state.
pub fn led_color_for(state: i32, kind: i32, selected: bool) -> (u8, u8) {
    if kind == 0 {
        return if selected {
            (COLOR_WHITE, LED_HALF)
        } else {
            (COLOR_GREY, LED_HALF)
        };
    }
    match state {
        1 => (COLOR_YELLOW, LED_BLINK),
        2 => match kind {
            1 => (COLOR_AMBER, LED_FULL),
            2 => (COLOR_BLUE, LED_FULL),
            3 => (COLOR_YELLOW, LED_FULL),
            _ => (COLOR_GREEN, LED_FULL),
        },
        3 => (COLOR_RED, LED_BLINK),
        _ => {
            if selected {
                (COLOR_WHITE, LED_FULL)
            } else {
                (COLOR_AMBER, LED_HALF)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MIDI stream parsing
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum MidiMessage {
    NoteOn {
        channel: u8,
        note: i32,
        velocity: i32,
    },
    NoteOff {
        channel: u8,
        note: i32,
        velocity: i32,
    },
    ControlChange {
        channel: u8,
        controller: i32,
        value: i32,
    },
    PitchBend {
        channel: u8,
        value: i32,
    },
    SysEx(Vec<u8>),
    Other,
}

/// Incremental MIDI 1.0 parser: USB packets split messages and controllers use
/// running status, so partial data bytes and the last channel status persist.
#[derive(Default)]
pub struct MidiStreamParser {
    sysex: Vec<u8>,
    data: [i32; 2],
    data_count: usize,
    running_status: u8,
    in_sysex: bool,
}

impl MidiStreamParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<MidiMessage> {
        let mut out = Vec::with_capacity(4);
        for byte in bytes {
            let byte = *byte;
            if byte == 0xF7 {
                if self.in_sysex {
                    self.sysex.push(byte);
                    out.push(MidiMessage::SysEx(std::mem::take(&mut self.sysex)));
                    self.in_sysex = false;
                }
                continue;
            }
            if byte == 0xF0 {
                self.sysex.clear();
                self.sysex.push(byte);
                self.in_sysex = true;
                self.data_count = 0;
                self.running_status = 0;
                continue;
            }
            if byte >= 0xF8 {
                continue;
            }
            if self.in_sysex {
                if byte & 0x80 == 0 {
                    self.sysex.push(byte);
                } else {
                    self.sysex.clear();
                    self.in_sysex = false;
                    self.running_status = byte;
                    self.data_count = 0;
                }
                continue;
            }
            if byte >= 0xF0 {
                self.running_status = 0;
                self.data_count = 0;
                continue;
            }
            if byte & 0x80 != 0 {
                self.running_status = byte;
                self.data_count = 0;
                continue;
            }
            if self.running_status == 0 {
                continue;
            }
            let needed = match self.running_status & 0xF0 {
                0xC0 | 0xD0 => 1,
                _ => 2,
            };
            self.data[self.data_count] = byte as i32;
            self.data_count += 1;
            if self.data_count >= needed {
                self.emit(&mut out);
                self.data_count = 0;
            }
        }
        out
    }

    fn emit(&mut self, out: &mut Vec<MidiMessage>) {
        let status = self.running_status;
        let channel = status & 0x0F;
        match status & 0xF0 {
            0x80 => out.push(MidiMessage::NoteOff {
                channel,
                note: self.data[0],
                velocity: self.data[1],
            }),
            0x90 => {
                if self.data[1] == 0 {
                    out.push(MidiMessage::NoteOff {
                        channel,
                        note: self.data[0],
                        velocity: 0,
                    });
                } else {
                    out.push(MidiMessage::NoteOn {
                        channel,
                        note: self.data[0],
                        velocity: self.data[1],
                    });
                }
            }
            0xB0 => out.push(MidiMessage::ControlChange {
                channel,
                controller: self.data[0],
                value: self.data[1],
            }),
            0xE0 => out.push(MidiMessage::PitchBend {
                channel,
                value: (self.data[0] | (self.data[1] << 7)) - 8192,
            }),
            _ => out.push(MidiMessage::Other),
        }
    }
}

/// A message tagged with the port it arrived on.
#[derive(Clone, Debug)]
pub struct IncomingMidi {
    pub port_index: usize,
    pub port_name: String,
    pub message: MidiMessage,
}

pub struct ApcState {
    pub connected: bool,
    pub firmware: Option<String>,
    pub mode: ApcPadMode,
    pub base_note: i32,
    pub flip_rows: bool,
}

impl Default for ApcState {
    fn default() -> Self {
        Self {
            connected: false,
            firmware: None,
            mode: ApcPadMode::Unknown,
            base_note: PAD_BASE_DAW,
            flip_rows: true,
        }
    }
}

pub struct MidiHub {
    connections: Vec<MidiInputConnection<()>>,
    output: Option<MidiOutputConnection>,
    tx: Sender<IncomingMidi>,
    rx: Receiver<IncomingMidi>,
    pub ports: Vec<String>,
    pub apc: ApcState,
    pub status: String,
    pub error: Option<String>,
    /// The last few incoming messages, formatted for the MIDI sheet.
    pub last_events: Vec<String>,
    /// Strip index from the most recent strip-touch note, if any.
    pub touched_strip: Option<usize>,
}

impl Default for MidiHub {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiHub {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        let ports = MidiInput::new("ReplicantSynth probe")
            .ok()
            .map(|input| {
                input
                    .ports()
                    .iter()
                    .filter_map(|port| input.port_name(port).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Self {
            connections: Vec::new(),
            output: None,
            tx,
            rx,
            ports,
            apc: ApcState::default(),
            status: "not connected".to_string(),
            error: None,
            last_events: Vec::new(),
            touched_strip: None,
        }
    }

    fn parser_reset(&mut self) {
        // Parsers live inside the connection callbacks; nothing to reset here
        // beyond the shared monitor state.
        self.touched_strip = None;
    }

    pub fn disconnect(&mut self) {
        self.connections.clear();
        self.output = None;
        self.parser_reset();
        self.apc.connected = false;
        self.apc.mode = ApcPadMode::Unknown;
        self.status = "not connected".to_string();
    }

    /// Open every port that matches `filter` (case-insensitive substring).
    ///
    /// Each port gets its own `MidiInput`, because midir's `connect` consumes the
    /// handle on failure; that keeps one broken cable from closing the others.
    pub fn connect(&mut self, filter: Option<&str>) -> Result<usize, String> {
        self.disconnect();
        let filter_upper = filter.map(|f| f.to_uppercase());

        // Snapshot the port list with a throwaway probe.
        let mut targets: Vec<(usize, String)> = Vec::new();
        if let Ok(probe) = MidiInput::new("ReplicantSynth probe") {
            for (index, port) in probe.ports().iter().enumerate() {
                let name = probe
                    .port_name(port)
                    .unwrap_or_else(|_| format!("port {index}"));
                let matches = match &filter_upper {
                    Some(needle) => name.to_uppercase().contains(needle),
                    None => true,
                };
                if matches {
                    targets.push((index, name));
                }
            }
        }
        self.ports = targets.iter().map(|(_, name)| name.clone()).collect();

        let mut opened = 0usize;
        self.error = None;
        for (index, name) in &targets {
            let input = match MidiInput::new("ReplicantSynth") {
                Ok(input) => input,
                Err(err) => {
                    self.error = Some(err.to_string());
                    break;
                }
            };
            let ports = input.ports();
            let Some(port) = ports.get(*index) else {
                continue;
            };
            let parser = MidiStreamParser::new();
            let tx = self.tx.clone();
            let port_name = name.clone();
            let callback_index = *index;
            let mut parser = parser;
            match input.connect(
                port,
                "replicantsynth-in",
                move |_stamp, bytes, _| {
                    for message in parser.feed(bytes) {
                        let _ = tx.send(IncomingMidi {
                            port_index: callback_index,
                            port_name: port_name.clone(),
                            message,
                        });
                    }
                },
                (),
            ) {
                Ok(connection) => {
                    self.connections.push(connection);
                    opened += 1;
                }
                Err(err) => {
                    self.error = Some(format!("{name}: {}", err.kind()));
                }
            }
        }

        // Open the first matching output, preferring the DAW cable.
        if let Ok(output) = MidiOutput::new("ReplicantSynth out") {
            let out_ports = output.ports();
            let mut chosen = None;
            for (index, port) in out_ports.iter().enumerate() {
                let name = output.port_name(port).unwrap_or_default();
                let matches = match &filter_upper {
                    Some(needle) => name.to_uppercase().contains(needle),
                    None => true,
                };
                if !matches {
                    continue;
                }
                let role = port_role(&name, index);
                if role == PortRole::Daw {
                    chosen = Some(port.clone());
                    break;
                }
                if chosen.is_none() {
                    chosen = Some(port.clone());
                }
            }
            if let Some(port) = chosen {
                match output.connect(&port, "replicantsynth-out") {
                    Ok(connection) => {
                        self.output = Some(connection);
                        for message in handshake() {
                            let _ = self.send(&message);
                        }
                    }
                    Err(err) => self.error = Some(err.kind().to_string()),
                }
            }
        }

        if opened == 0 {
            self.status = "no matching MIDI ports".to_string();
            return Err(self.status.clone());
        }

        self.apc.connected = true;
        let names: Vec<String> = targets.iter().map(|(_, name)| name.clone()).collect();
        self.status = format!("{opened} port(s): {}", names.join(" · "));
        Ok(opened)
    }

    pub fn is_connected(&self) -> bool {
        !self.connections.is_empty()
    }

    /// Human-readable one-line summary of a message, for the monitor.
    pub fn describe(message: &MidiMessage) -> String {
        match message {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => format!("note on  ch{} {note} vel {velocity}", channel + 1),
            MidiMessage::NoteOff { channel, note, .. } => {
                format!("note off ch{} {note}", channel + 1)
            }
            MidiMessage::ControlChange {
                channel,
                controller,
                value,
            } => format!("cc       ch{} {controller} = {value}", channel + 1),
            MidiMessage::PitchBend { channel, value } => {
                format!("bend     ch{} {value:+}", channel + 1)
            }
            MidiMessage::SysEx(bytes) => format!("sysex    {} bytes", bytes.len()),
            MidiMessage::Other => "other".to_string(),
        }
    }

    pub fn record_event(&mut self, message: &MidiMessage) {
        let line = Self::describe(message);
        self.last_events.push(line);
        if self.last_events.len() > 80 {
            self.last_events.remove(0);
        }
    }

    /// Blank every pad LED, used when a session ends or on demand.
    pub fn clear_pads(&mut self) {
        for pad in 0..PAD_COUNT {
            let note = note_for_pad(pad, self.apc.base_note, self.apc.flip_rows);
            self.send(&led(note, COLOR_OFF, LED_HALF)).ok();
        }
    }

    pub fn poll(&self) -> Vec<IncomingMidi> {
        self.rx.try_iter().collect()
    }

    pub fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        match &mut self.output {
            Some(output) => output.send(bytes).map_err(|err| err.to_string()),
            None => Err("no MIDI output".to_string()),
        }
    }

    pub fn send_oled(&mut self, line: usize, text: &str) {
        let message = oled_line(line, text);
        let _ = self.send(&message);
    }

    pub fn send_pad_led(&mut self, pad: usize, color: u8, brightness: u8) {
        let note = note_for_pad(pad, self.apc.base_note, self.apc.flip_rows);
        let _ = self.send(&led(note, color, brightness));
    }

    pub fn send_strip_led(&mut self, strip: usize, value: f32, bipolar: bool) {
        if strip >= STRIP_COUNT {
            return;
        }
        let mode = if bipolar { 3 } else { 1 };
        let _ = self.send(&cc(CC_STRIP_MODE_FIRST + strip as u8, mode));
        let _ = self.send(&cc(CC_STRIP_COLOR_FIRST + strip as u8, 3));
        let position = (value.clamp(0.0, 1.0) * 16383.0) as i32;
        let _ = self.send(&pitch_bend(strip as u8, position));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_mapping_round_trips() {
        for pad in 0..PAD_COUNT {
            let note = note_for_pad(pad, PAD_BASE_DAW, true);
            assert_eq!(pad_for_note(note, PAD_BASE_DAW, true), Some(pad));
        }
        // note 0 is the bottom-left pad, i.e. index 56 in a top-left grid.
        assert_eq!(pad_for_note(0, PAD_BASE_DAW, true), Some(56));
        assert_eq!(note_for_pad(56, PAD_BASE_DAW, true), 0);
    }

    #[test]
    fn sysex_handshake_round_trips() {
        let message = mode_message(0);
        assert_eq!(
            message,
            vec![0xF0, 0x47, 0x00, 0x53, 0x19, 0x00, 0x01, 0x00, 0xF7]
        );
        assert_eq!(pad_mode_from_sysex(&message), Some(ApcPadMode::Session));

        let step = mode_message(6);
        assert_eq!(pad_mode_from_sysex(&step), Some(ApcPadMode::Step));

        let (id, payload) = parse_akai_message(&message).expect("envelope");
        assert_eq!(id, 0x19);
        assert_eq!(payload, &[0x00]);
    }

    #[test]
    fn identity_reply_yields_firmware() {
        let reply = [
            0xF0, 0x7E, 0x00, 0x06, 0x02, 0x47, 0x53, 0x00, 0x19, 1, 2, 1, 6, 0, 0, 0, 0xF7,
        ];
        assert_eq!(firmware_from_identity(&reply), Some("1.2.1.6".to_string()));
        assert!(firmware_supported(Some("1.2.1.6")));
        assert!(firmware_supported(Some("1.3.0.0")));
        assert!(!firmware_supported(Some("1.2.0.9")));
        assert!(!firmware_supported(None));
    }

    #[test]
    fn oled_line_is_an_akai_message() {
        let message = oled_line(0, "REPLICANT");
        let (id, payload) = parse_akai_message(&message).expect("envelope");
        assert_eq!(id, 0x10);
        assert_eq!(payload[0], 0);
        assert_eq!(payload[1], b'R');
        assert_eq!(payload.len(), 10); // line + 8 chars + terminator
    }

    #[test]
    fn parser_handles_running_status_and_split_messages() {
        let mut parser = MidiStreamParser::new();
        // Two note-ons sharing one status byte, then a split control change.
        let first = parser.feed(&[0x90, 36, 100, 38, 90]);
        assert_eq!(first.len(), 2);
        assert!(matches!(
            first[0],
            MidiMessage::NoteOn {
                note: 36,
                velocity: 100,
                ..
            }
        ));
        let second = parser.feed(&[0xB0, 90]);
        assert!(second.is_empty());
        let third = parser.feed(&[5]);
        assert!(matches!(
            third[0],
            MidiMessage::ControlChange {
                controller: 90,
                value: 5,
                ..
            }
        ));
    }

    #[test]
    fn parser_decodes_sysex_across_packets() {
        let mut parser = MidiStreamParser::new();
        assert!(parser.feed(&[0xF0, 0x47, 0x00]).is_empty());
        let messages = parser.feed(&[0x53, 0x19, 0x00, 0x01, 0x00, 0xF7]);
        assert_eq!(messages.len(), 1);
        match &messages[0] {
            MidiMessage::SysEx(bytes) => {
                assert_eq!(pad_mode_from_sysex(bytes), Some(ApcPadMode::Session));
            }
            other => panic!("expected sysex, got {other:?}"),
        }
    }

    #[test]
    fn note_on_with_zero_velocity_is_a_note_off() {
        let mut parser = MidiStreamParser::new();
        let messages = parser.feed(&[0x90, 60, 0]);
        assert!(matches!(messages[0], MidiMessage::NoteOff { note: 60, .. }));
    }

    #[test]
    fn led_palette_covers_every_clip_state() {
        assert_eq!(led_color_for(2, 1, false), (COLOR_AMBER, LED_FULL));
        assert_eq!(led_color_for(2, 2, false), (COLOR_BLUE, LED_FULL));
        assert_eq!(led_color_for(2, 3, false), (COLOR_YELLOW, LED_FULL));
        assert_eq!(led_color_for(2, 4, false), (COLOR_GREEN, LED_FULL));
        assert_eq!(led_color_for(1, 1, false), (COLOR_YELLOW, LED_BLINK));
        assert_eq!(led_color_for(3, 1, false), (COLOR_RED, LED_BLINK));
        assert_eq!(led_color_for(0, 0, false), (COLOR_GREY, LED_HALF));
        assert_eq!(led_color_for(0, 0, true), (COLOR_WHITE, LED_HALF));
    }

    #[test]
    fn scene_and_strip_notes_map() {
        assert_eq!(scene_for_note(112), Some(0));
        assert_eq!(scene_for_note(119), Some(7));
        assert_eq!(scene_for_note(120), None);
        assert_eq!(strip_touch_for_note(82), Some(0));
        assert_eq!(strip_touch_for_note(89), Some(7));
        assert_eq!(arrow_for_note(94), Some(Arrow::Up));
        assert_eq!(arrow_for_note(97), Some(Arrow::Right));
    }

    #[test]
    fn port_roles_prefer_names_then_index() {
        assert_eq!(port_role("APC64 DAW", 2), PortRole::Daw);
        assert_eq!(port_role("APC64 Notes", 0), PortRole::Notes);
        assert_eq!(port_role("APC64 Custom", 0), PortRole::Custom);
        assert_eq!(port_role("APC64 MIDI", 0), PortRole::Midi);
        assert_eq!(port_role("Port 1", 1), PortRole::Notes);
        assert_eq!(port_role("Port 9", 9), PortRole::Unknown);
    }
}
