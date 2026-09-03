use std::fmt;

pub const BAUD_RATE: u32 = 921_600;
pub const HEADER_LEN: usize = 6;
pub const MAX_FRAME_LEN: usize = 1_048_576;

pub mod opcode {
    pub const STANDBY: u8 = 0x00;
    pub const STATION_UP: u8 = 0x01;
    pub const STATION_DOWN: u8 = 0x02;
    pub const TUNE_TO_STATION_ID: u8 = 0x03;
    pub const TUNE_TO_FREQUENCY: u8 = 0x04;
    pub const SET_MODE: u8 = 0x05;
    pub const BAND_UP: u8 = 0x06;
    pub const VOLUME_UP: u8 = 0x07;
    pub const VOLUME_DOWN: u8 = 0x08;
    pub const GET_SCANNED_STATIONS: u8 = 0x09;
    pub const GET_STATUS: u8 = 0x0a;
    pub const STATION_RECALL: u8 = 0x0b;
    pub const STATION_STORE: u8 = 0x0c;
    pub const CMX918_REG_READ: u8 = 0x0d;
    pub const CMX918_REG_WRITE: u8 = 0x0e;
    pub const STATUS_OUTPUT: u8 = 0x0f;
    pub const GET_VERSION: u8 = 0x10;
    pub const GET_TUNED_FREQ: u8 = 0x14;
    pub const GET_DEMOD_MODE: u8 = 0x15;
    pub const STATION_UP_CHANNEL_RASTER: u8 = 0x18;
    pub const STATION_DOWN_CHANNEL_RASTER: u8 = 0x19;
    pub const START_SCAN: u8 = 0x1a;
    pub const GET_VOLUME: u8 = 0x1d;
    pub const SET_VOLUME: u8 = 0x1e;
    pub const STATION_GET_ID: u8 = 0x1f;
    pub const TEXT_MSG_OUT: u8 = 0x20;
    pub const TEXT_MSG_SERVICE: u8 = 0x21;
    pub const RSSI_GET: u8 = 0x22;
    pub const SET_SCREEN_STATUS: u8 = 0x25;
    pub const GET_SCREEN_STATUS: u8 = 0x26;
    pub const GET_SCANNER_PROGRESS: u8 = 0x27;
    pub const AUDIO_TEST_TONE: u8 = 0x60;
    pub const UART_INIT: u8 = 0x7f;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Drm,
    AmWide,
    AmNarrow,
    Fm,
    Scanner,
    AfsSearch,
    EmergencyWarning,
}

impl Mode {
    pub fn from_status(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Drm,
            1 => Self::AmWide,
            2 => Self::AmNarrow,
            3 => Self::Fm,
            4 => Self::Scanner,
            5 => Self::AfsSearch,
            6 => Self::EmergencyWarning,
            _ => return None,
        })
    }

    pub fn set_value(self) -> Option<u8> {
        match self {
            Self::Drm => Some(0),
            Self::AmWide | Self::Fm => Some(1),
            Self::AmNarrow => Some(2),
            _ => None,
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Drm => "DRM",
            Self::AmWide => "AM wide",
            Self::AmNarrow => "AM narrow",
            Self::Fm => "FM",
            Self::Scanner => "Scanner",
            Self::AfsSearch => "AFS search",
            Self::EmergencyWarning => "Emergency warning",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub opcode: u8,
    pub parameters: Vec<u8>,
}

impl Request {
    pub fn new(opcode: u8) -> Self {
        Self {
            opcode,
            parameters: Vec::new(),
        }
    }

    pub fn byte(opcode: u8, value: u8) -> Self {
        Self {
            opcode,
            parameters: vec![value],
        }
    }

    pub fn word(opcode: u8, value: u32) -> Self {
        Self {
            opcode,
            parameters: value.to_le_bytes().to_vec(),
        }
    }

    pub fn bytes(opcode: u8, values: &[u8]) -> Self {
        Self {
            opcode,
            parameters: values.to_vec(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(1 + self.parameters.len());
        bytes.push(self.opcode);
        bytes.extend_from_slice(&self.parameters);
        bytes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub opcode: u8,
    pub error: u8,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn require_ok(&self) -> Result<(), String> {
        if self.error == 0 {
            Ok(())
        } else {
            Err(error_description(self.error).to_owned())
        }
    }
}

#[derive(Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    pub fn next_frame(&mut self) -> Option<Frame> {
        while self.buffer.len() >= HEADER_LEN {
            if !is_response_opcode(self.buffer[0]) || self.buffer[1] > 18 {
                self.buffer.remove(0);
                continue;
            }
            let length = u32::from_le_bytes(self.buffer[2..6].try_into().ok()?) as usize;
            if !(HEADER_LEN..=MAX_FRAME_LEN).contains(&length) {
                self.buffer.remove(0);
                continue;
            }
            if self.buffer.len() < length {
                return None;
            }
            let bytes: Vec<u8> = self.buffer.drain(..length).collect();
            return Some(Frame {
                opcode: bytes[0],
                error: bytes[1],
                payload: bytes[6..].to_vec(),
            });
        }
        None
    }
}

fn is_response_opcode(value: u8) -> bool {
    matches!(
        value,
        0x00..=0x16 | 0x18..=0x27 | 0x50..=0x51 | 0x60..=0x62 | 0x7f
    )
}

#[derive(Clone, Debug, Default)]
pub struct Service {
    pub id: u32,
    pub short_id: u8,
    pub label: String,
    pub is_data: bool,
    pub text_available: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ReceiverStatus {
    pub mode: Mode,
    pub frame_synced: bool,
    pub signal_detected: bool,
    pub fac_crc_valid: bool,
    pub sdc_crc_valid: bool,
    pub msc_crc_valid: bool,
    pub fac_mer_db: f32,
    pub sdc_mer_db: f32,
    pub msc_mer_db: f32,
    pub robustness_mode: u8,
    pub sample_rate_offset_ppm: f32,
    pub frequency_offset: f32,
    pub services: Vec<Service>,
    pub frame_rms_dbfs: f32,
    pub rssi_dbm: f32,
}

pub fn parse_status(frame: &Frame) -> Result<ReceiverStatus, String> {
    frame.require_ok()?;
    let data = &frame.payload;
    if data.len() != 409 {
        return Err(format!(
            "status payload is {} bytes, expected 409",
            data.len()
        ));
    }
    let service_count = data[45].min(4) as usize;
    let mut services = Vec::new();
    for index in 0..service_count {
        let base = 46 + index * 84;
        let id = le_u32(data, base)? & 0x000f_ffff;
        let label_length = usize::from(data[base + 83]).min(65);
        let label = String::from_utf8_lossy(&data[base + 18..base + 18 + label_length])
            .trim_end_matches('\0')
            .to_owned();
        services.push(Service {
            id,
            short_id: data[base + 4],
            label,
            is_data: data[base + 6] != 0,
            text_available: data[base + 14] != 0,
        });
    }
    let tail = 46 + 4 * 84;
    Ok(ReceiverStatus {
        mode: Mode::from_status(data[0]).ok_or_else(|| format!("unknown mode {}", data[0]))?,
        frame_synced: data[1] != 0,
        signal_detected: data[2] != 0,
        fac_crc_valid: data[3] != 0,
        fac_mer_db: le_f32(data, 4)?,
        sdc_crc_valid: data[12] != 0,
        sdc_mer_db: le_f32(data, 13)?,
        msc_crc_valid: data[21] != 0,
        msc_mer_db: le_f32(data, 22)?,
        robustness_mode: data[30],
        sample_rate_offset_ppm: le_f32(data, 37)?,
        frequency_offset: le_i32(data, 41)? as f32 / 65_536.0,
        services,
        frame_rms_dbfs: le_f32(data, tail + 16)?,
        rssi_dbm: le_f32(data, tail + 20)?,
    })
}

#[derive(Clone, Debug, Default)]
pub struct ScannedStation {
    pub mode: Mode,
    pub frequency_hz: u32,
    pub services: Vec<Service>,
}

pub fn parse_scanned_stations(frame: &Frame) -> Result<Vec<ScannedStation>, String> {
    frame.require_ok()?;
    if frame.payload.len() < 4 {
        return Err("station response is too short".to_owned());
    }
    let count = le_u32(&frame.payload, 0)? as usize;
    if count > 100 || frame.payload.len() < 4 + count * 24 {
        return Err("invalid station count or truncated station table".to_owned());
    }
    let labels_data = &frame.payload[4 + count * 24..];
    let labels: Vec<String> = labels_data
        .split(|byte| *byte == 0)
        .map(|bytes| String::from_utf8_lossy(bytes).to_string())
        .collect();
    let mut stations = Vec::with_capacity(count);
    for index in 0..count {
        let base = 4 + index * 24;
        let mode_freq = le_u32(&frame.payload, base)?;
        let mode = Mode::from_status((mode_freq >> 30) as u8)
            .ok_or_else(|| "invalid scanned station mode".to_owned())?;
        let service_count = usize::from(frame.payload[base + 20].min(4));
        let mut services = Vec::new();
        for service_index in 0..service_count {
            let packed = le_u32(&frame.payload, base + 4 + service_index * 4)?;
            let label_index = ((packed >> 25) & 0x7f) as usize;
            services.push(Service {
                id: packed & 0x00ff_ffff,
                label: labels.get(label_index).cloned().unwrap_or_default(),
                is_data: packed & (1 << 24) != 0,
                ..Default::default()
            });
        }
        stations.push(ScannedStation {
            mode,
            frequency_hz: mode_freq & 0x3fff_ffff,
            services,
        });
    }
    Ok(stations)
}

pub fn parse_text_message(frame: &Frame) -> Result<Option<String>, String> {
    frame.require_ok()?;
    if frame.payload.len() < 2 {
        return Err("text message response is too short".to_owned());
    }
    if frame.payload[0] == 1 {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8_lossy(&frame.payload[2..]).to_string(),
    ))
}

pub fn le_u32(data: &[u8], offset: usize) -> Result<u32, String> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| "truncated u32 field".to_owned())?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn le_i32(data: &[u8], offset: usize) -> Result<i32, String> {
    Ok(le_u32(data, offset)? as i32)
}

fn le_f32(data: &[u8], offset: usize) -> Result<f32, String> {
    Ok(f32::from_bits(le_u32(data, offset)?))
}

pub fn error_description(code: u8) -> &'static str {
    match code {
        0 => "no error",
        3 => "invalid frequency",
        4 => "no station found",
        5 => "station load failed: no station",
        6 => "station load failed: flash error",
        7 => "station store failed: flash error",
        8 => "station store failed: current station invalid",
        9 => "persistent configuration contains an invalid parameter",
        10 => "persistent configuration write failed",
        11 => "persistent configuration is invalid",
        12 => "persistent configuration read failed",
        13 => "scanner is in progress",
        14 => "invalid demodulation mode",
        15 => "invalid volume",
        16 => "screen not present",
        17 => "scanner is not in progress",
        18 => "buttons disabled in headless mode",
        _ => "unknown module error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(opcode: u8, error: u8, payload: &[u8]) -> Vec<u8> {
        let mut result = vec![opcode, error];
        result.extend_from_slice(&((HEADER_LEN + payload.len()) as u32).to_le_bytes());
        result.extend_from_slice(payload);
        result
    }

    #[test]
    fn request_words_are_little_endian() {
        assert_eq!(
            Request::word(opcode::TUNE_TO_FREQUENCY, 1_000_000).encode(),
            vec![4, 0x40, 0x42, 0x0f, 0]
        );
    }

    #[test]
    fn decoder_handles_fragmented_frames_and_noise() {
        let bytes = response(opcode::GET_VOLUME, 0, &[72]);
        let mut decoder = FrameDecoder::default();
        decoder.push(b"OK");
        decoder.push(&bytes[..3]);
        assert!(decoder.next_frame().is_none());
        decoder.push(&bytes[3..]);
        let frame = decoder.next_frame().unwrap();
        assert_eq!(frame.opcode, opcode::GET_VOLUME);
        assert_eq!(frame.payload, vec![72]);
    }

    #[test]
    fn parses_key_status_fields() {
        let mut payload = vec![0; 409];
        payload[0] = 0;
        payload[1] = 1;
        payload[2] = 1;
        payload[3] = 1;
        payload[4..8].copy_from_slice(&18.5f32.to_le_bytes());
        payload[45] = 1;
        payload[46..50].copy_from_slice(&0x12345u32.to_le_bytes());
        payload[50] = 2;
        payload[60] = 1;
        payload[64..68].copy_from_slice(b"Test");
        payload[129] = 4;
        let tail = 382;
        payload[tail + 20..tail + 24].copy_from_slice(&(-73.25f32).to_le_bytes());
        let frame = Frame {
            opcode: opcode::GET_STATUS,
            error: 0,
            payload,
        };
        let status = parse_status(&frame).unwrap();
        assert_eq!(status.mode, Mode::Drm);
        assert!(status.frame_synced);
        assert_eq!(status.services[0].label, "Test");
        assert_eq!(status.rssi_dbm, -73.25);
    }

    #[test]
    fn scanned_station_label_indexes_keep_empty_entries() {
        let mut payload = 1u32.to_le_bytes().to_vec();
        payload.extend_from_slice(&(1_000_000u32).to_le_bytes());
        payload.extend_from_slice(&((1u32 << 25) | 0x123456).to_le_bytes());
        payload.extend_from_slice(&[0; 12]);
        payload.push(1);
        payload.extend_from_slice(&[0; 3]);
        payload.extend_from_slice(b"\0Radio One\0");
        let frame = Frame {
            opcode: opcode::GET_SCANNED_STATIONS,
            error: 0,
            payload,
        };
        let stations = parse_scanned_stations(&frame).unwrap();
        assert_eq!(stations[0].services[0].label, "Radio One");
    }
}
