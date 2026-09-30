use base64::Engine as _;

/// The one format capture speaks. whisper's mel transform assumes 16 kHz and
/// the frontend resamples for free in the AudioContext, so a clip that arrives
/// at another rate is dropped rather than decoded into noise.
pub const SAMPLE_RATE: u32 = 16_000;
pub const CHANNELS: u16 = 1;
pub const BITS: u16 = 16;

const HEADER_SZ: usize = 44;
const BYTE_RATE: u32 = SAMPLE_RATE * 2;

/// Wrap mono 16-bit samples in a WAV container. The inverse of `decode`, and
/// what the tests build their fixtures from.
pub fn encode(samples: &[i16]) -> Vec<u8> {
    let data_sz = (samples.len() * 2) as u32;
    let mut buf = Vec::with_capacity(HEADER_SZ + samples.len() * 2);

    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_sz).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16_u32.to_le_bytes());
    buf.extend_from_slice(&1_u16.to_le_bytes());
    buf.extend_from_slice(&CHANNELS.to_le_bytes());
    buf.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    buf.extend_from_slice(&BYTE_RATE.to_le_bytes());
    buf.extend_from_slice(&(CHANNELS * BITS / 8).to_le_bytes());
    buf.extend_from_slice(&BITS.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_sz.to_le_bytes());

    for s in samples {
        buf.extend_from_slice(&s.to_le_bytes());
    }

    buf
}

/// Pull the samples back out of a WAV container.
///
/// Every length in the header is attacker-controlled: this decodes a string
/// that came over IPC. So the declared sizes are checked against what actually
/// arrived before a single sample is read.
pub fn decode(bytes: &[u8]) -> Result<Vec<i16>, String> {
    chk(
        bytes.len() >= HEADER_SZ,
        "audio is too short to be a wav file",
    )?;

    chk(&bytes[0..4] == b"RIFF", "audio is not a wav file")?;
    chk(&bytes[8..12] == b"WAVE", "audio is not a wav file")?;
    chk(
        &bytes[12..16] == b"fmt ",
        "audio is missing its format chunk",
    )?;

    let rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let channels = u16::from_le_bytes(bytes[22..24].try_into().unwrap());
    let bits = u16::from_le_bytes(bytes[34..36].try_into().unwrap());

    // Loud fail. whisper assumes 16 kHz mono and does not check, so a 48 kHz
    // clip decodes to confident garbage instead of an error.
    chk(
        rate == SAMPLE_RATE,
        &format!("audio is {rate} Hz; dictation needs {SAMPLE_RATE} Hz"),
    )?;
    chk(
        channels == CHANNELS,
        &format!("audio has {channels} channels; dictation is mono"),
    )?;
    chk(
        bits == BITS,
        &format!("audio is {bits}-bit; dictation is {BITS}-bit"),
    )?;

    chk(
        &bytes[36..40] == b"data",
        "audio is missing its sample chunk",
    )?;
    let data_sz = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
    let body = &bytes[HEADER_SZ..];

    // The header can claim four gigabytes that never arrived.
    chk(
        data_sz <= body.len(),
        "audio is truncated: its header declares more samples than were sent",
    )?;
    chk(data_sz.is_multiple_of(2), "audio has a partial sample")?;

    Ok(body[..data_sz]
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect())
}

/// IPC hands us a string, because a byte array would arrive as a JSON array of
/// numbers. Decode it here so the guard against a lying header runs on the real
/// bytes rather than on whatever the string claimed.
pub fn decode_b64(b64: &str) -> Result<Vec<i16>, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|err| format!("audio is not valid base64: {err}"))?;

    decode(&bytes)
}

/// Normalised by 32768, not `i16::MAX`. The two ends are one sample apart and
/// the asymmetry is real: dividing by 32767 puts `i16::MIN` at -1.00003, so the
/// quietest full-scale sample clips outside the range whisper expects.
const FULL_SCALE: f32 = 32_768.0;

/// whisper wants normalised floats, not integers.
pub fn to_f32(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|s| *s as f32 / FULL_SCALE).collect()
}

pub fn duration_ms(samples: &[i16]) -> i64 {
    samples.len() as i64 * 1000 / SAMPLE_RATE as i64
}

/// Loudest sample, 0.0 to 1.0. The silence gate reads this rather than an RMS,
/// because a single breath above the floor is enough to be worth decoding.
pub fn peak(samples: &[i16]) -> f32 {
    samples
        .iter()
        .map(|s| (*s as f32 / FULL_SCALE).abs())
        .fold(0.0_f32, f32::max)
}

fn chk(ok: bool, msg: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(msg.to_string())
    }
}
