use super::wav;

/// The hard cap on one utterance. Bounded for the same reason a provider stall
/// is: unbounded capture is unbounded memory, and there is nothing to size it
/// against.
pub const MAX_SECS: i64 = 60;

/// Below this peak there is no speech worth decoding. Deliberately low — a
/// quiet room sits near 0.02 and a murmur near 0.1 — because the cost of being
/// wrong here is only a wasted decode, not a lost word.
pub const SILENCE_FLOOR: f32 = 0.02;

/// A turn that survives VAD and still comes back as one of these is whisper
/// talking to itself. VAD is the real defense; this is the backstop for when
/// the VAD model is not on disk yet, so it stays blunt. Dropping a genuine
/// one-word "yes" is the cost, and that is the safe direction to be wrong in.
const ARTIFACTS: &[&str] = &[
    "you",
    "thank you",
    "thanks",
    "thanks for watching",
    "thank you for watching",
    "please subscribe",
    "subscribe",
    "subtitles by the amara org community",
    "subtitles by amara org",
    "amara org",
    "subtitled by",
    "transcription by castingwords",
    "transcript by castingwords",
    "transcribed by",
    "captioned by",
    "c subs dot com",
    "blank audio",
    "silence",
    "music",
    "upbeat music",
    "background music",
    "soft music",
    "applause",
    "laughter",
    "bye",
    "bye bye",
    "okay",
    "ok",
    "yeah",
    "yep",
    "yes",
    "no",
    "mm hmm",
    "hmm",
    "uh",
    "um",
    "ah",
    "oh",
    "wow",
    "so",
    "what",
    "why",
    "who",
    "how",
    "where",
    "when",
];

/// What to do with a finished turn. Distinct cases because the composer says a
/// different thing for each: "I didn't hear anything" and "that sounded like
/// noise" are not the same failure, and neither is "that was too long".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Keep,
    Silent,
    Artifact,
    TooLong,
    Empty,
}

/// Run every gate. `precheck` first, then the decode, then `postcheck` — the
/// two halves are split so a runaway clip is refused before it costs a model
/// load and a decode.
pub fn check(samples: &[i16], text: &str) -> Verdict {
    if let Some(v) = precheck(samples) {
        return v;
    }

    postcheck(text).unwrap_or(Verdict::Keep)
}

/// The gates that only need the audio. Cheap enough to run first.
pub fn precheck(samples: &[i16]) -> Option<Verdict> {
    if wav::duration_ms(samples) > MAX_SECS * 1000 {
        return Some(Verdict::TooLong);
    }

    if samples.is_empty() || wav::peak(samples) < SILENCE_FLOOR {
        return Some(Verdict::Silent);
    }

    None
}

/// The gates that need the transcript, so they can only run after the decode.
pub fn postcheck(text: &str) -> Option<Verdict> {
    let norm = normalize(text);
    if norm.is_empty() {
        return Some(Verdict::Empty);
    }

    if ARTIFACTS.contains(&norm.as_str()) || loops(text) {
        return Some(Verdict::Artifact);
    }

    None
}

pub fn verdict_msg(v: Verdict) -> &'static str {
    match v {
        Verdict::Keep => "",
        Verdict::Silent => "I didn't hear anything.",
        Verdict::Artifact => "That sounded like noise, not speech — nothing was added.",
        Verdict::TooLong => "That was longer than a minute. Try a shorter one.",
        Verdict::Empty => "I didn't hear any words in that.",
    }
}

/// Lowercase, keep letters and digits, collapse whitespace. Punctuation and
/// brackets go, so "[BLANK_AUDIO]" and "Blank audio." land on the same key.
///
/// `_` and `-` are separators rather than dropped: whisper writes the bracket
/// tags with underscores and the interjections with hyphens, and deleting them
/// glues the words together ("blankaudio") where the table spells them with a
/// space.
fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for c in text.chars() {
        if c.is_whitespace() || c == '_' || c == '-' {
            if !out.ends_with(' ') {
                out.push(' ');
            }
        } else if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        }
    }

    out.trim().to_string()
}

/// whisper's other failure is looping — "Thank you. Thank you. Thanks for
/// watching." The variants differ, so matching identical sentences alone misses
/// it; three or more sentences drawn from at most two distinct ones is the
/// shape that actually shows up.
fn loops(text: &str) -> bool {
    let parts: Vec<String> = text
        .split(['.', '!', '?', '\n'])
        .map(normalize)
        .filter(|p| !p.is_empty())
        .collect();

    if parts.len() < 3 {
        return false;
    }

    let mut distinct: Vec<&str> = Vec::new();
    for p in &parts {
        if !distinct.contains(&p.as_str()) {
            distinct.push(p.as_str());
        }
    }

    distinct.len() <= 2
}
