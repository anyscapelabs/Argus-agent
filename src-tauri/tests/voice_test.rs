use argus_lib::voice::filter::{self, Verdict};
use argus_lib::voice::wav;

/// Loud enough to clear the silence floor without being a real signal.
fn speech(ms: i64) -> Vec<i16> {
    vec![8_000; 16_000 * ms as usize / 1000]
}

fn wav_of(samples: &[i16]) -> Vec<u8> {
    wav::encode(samples)
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}

// --- wav -----------------------------------------------------------------

#[test]
fn header_describes_mono_16k_16bit() {
    let b = wav_of(&[0, 1, 2]);

    assert_eq!(b.len(), 44 + 6, "header plus three i16 samples");
    assert_eq!(&b[0..4], b"RIFF");
    assert_eq!(&b[8..12], b"WAVE");
    assert_eq!(&b[12..16], b"fmt ");
    assert_eq!(le16(&b, 20), 1, "PCM, not float");
    assert_eq!(le16(&b, 22), 1, "mono");
    assert_eq!(le32(&b, 24), 16_000, "whisper assumes 16 kHz");
    assert_eq!(le32(&b, 28), 32_000, "rate * channels * bytes");
    assert_eq!(le16(&b, 32), 2, "block align");
    assert_eq!(le16(&b, 34), 16, "bits per sample");
    assert_eq!(&b[36..40], b"data");
    assert_eq!(le32(&b, 40), 6, "data size excludes the header");
    assert_eq!(le32(&b, 4), 36 + 6, "RIFF size is everything after it");
}

#[test]
fn samples_survive_a_roundtrip() {
    let samples: Vec<i16> = vec![0, -1, 1, i16::MIN, i16::MAX, 12_345];
    assert_eq!(wav::decode(&wav_of(&samples)).unwrap(), samples);
}

#[test]
fn an_empty_clip_is_valid_but_empty() {
    let out = wav::decode(&wav_of(&[])).unwrap();
    assert!(out.is_empty());
    assert_eq!(wav::duration_ms(&out), 0);
}

#[test]
fn a_lying_data_size_cannot_overrun_the_buffer() {
    // The header is attacker-controlled: it comes over IPC from a string.
    let mut b = wav_of(&[1, 2, 3]);
    b[40..44].copy_from_slice(&u32::MAX.to_le_bytes());

    let err = wav::decode(&b).unwrap_err();
    assert!(err.contains("truncated"), "got {err}");
}

#[test]
fn a_lying_data_size_cannot_shrink_the_clip() {
    let mut b = wav_of(&[1, 2, 3, 4, 5]);
    b[40..44].copy_from_slice(&2_u32.to_le_bytes());

    assert_eq!(wav::decode(&b).unwrap().len(), 1, "only what was declared");
}

#[test]
fn a_wrong_sample_rate_is_refused_not_decoded() {
    // whisper does not check. A 48 kHz clip would decode to confident noise.
    let mut b = wav_of(&[1, 2, 3]);
    b[24..28].copy_from_slice(&48_000_u32.to_le_bytes());

    let err = wav::decode(&b).unwrap_err();
    assert!(err.contains("48000 Hz"), "got {err}");
}

#[test]
fn stereo_is_refused() {
    let mut b = wav_of(&[1, 2, 3]);
    b[22..24].copy_from_slice(&2_u16.to_le_bytes());

    let err = wav::decode(&b).unwrap_err();
    assert!(err.contains("mono"), "got {err}");
}

#[test]
fn a_short_clip_is_refused() {
    let err = wav::decode(b"RIFF____WAVE").unwrap_err();
    assert!(err.contains("too short"), "got {err}");
}

#[test]
fn a_non_wav_clip_is_refused() {
    let mut b = wav_of(&[1, 2, 3]);
    b[0..4].copy_from_slice(b"OggS");

    let err = wav::decode(&b).unwrap_err();
    assert!(err.contains("not a wav"), "got {err}");
}

#[test]
fn base64_is_decoded_before_the_header_is_read() {
    use base64::Engine as _;

    let b64 = base64::engine::general_purpose::STANDARD.encode(wav_of(&[7, 8, 9]));
    assert_eq!(wav::decode_b64(&b64).unwrap(), vec![7, 8, 9]);

    let err = wav::decode_b64("not base64!!").unwrap_err();
    assert!(err.contains("base64"), "got {err}");
}

#[test]
fn floats_are_normalised() {
    let out = wav::to_f32(&[0, i16::MIN, i16::MAX, 16_383]);

    assert_eq!(out[0], 0.0);
    assert_eq!(out[1], -1.0);
    assert!(out[2] > 0.999, "full scale should still read as full scale");
    assert!(out[3] < 1.0, "no i16 should exceed full scale");

    // Every possible sample, not four chosen ones: the ends are one apart and
    // dividing by i16::MAX instead of 32768 pushes the low end past -1.0.
    for s in [i16::MIN, -1, 0, 1, i16::MAX] {
        let f = wav::to_f32(&[s])[0];
        assert!((-1.0..=1.0).contains(&f), "{s} landed at {f}");
    }
}

#[test]
fn the_gate_and_the_decoder_scale_the_amplitude_the_same_way() {
    // peak() decides silence and to_f32() feeds whisper. Two divisors would
    // mean a clip the gate calls loud decodes quieter than it was measured.
    let s = [i16::MAX];
    assert_eq!(wav::peak(&s), wav::to_f32(&s)[0].abs());
}

#[test]
fn peak_is_the_loudest_sample_not_the_average() {
    assert_eq!(wav::peak(&[]), 0.0);
    assert_eq!(wav::peak(&[100, -32_767, 10]), 32_767.0 / 32_768.0);
    assert_eq!(wav::peak(&[-8_000, 8_000]), 8_000.0 / 32_768.0);
}

#[test]
fn duration_counts_samples_at_the_capture_rate() {
    assert_eq!(wav::duration_ms(&vec![0; 16_000]), 1_000);
    assert_eq!(wav::duration_ms(&vec![0; 8_000]), 500);
    assert_eq!(wav::duration_ms(&[]), 0);
}

// --- filter --------------------------------------------------------------

#[test]
fn a_real_sentence_is_kept() {
    let s = speech(2_000);
    assert_eq!(
        filter::check(&s, "delete the production database"),
        Verdict::Keep
    );
}

#[test]
fn an_artifact_inside_a_real_sentence_is_not_dropped() {
    // Exact match only. Substring matching would eat half of real dictation.
    let s = speech(2_000);
    for txt in [
        "yes delete the production database",
        "thank you, now run the tests",
        "the amara org community",
        "so what",
    ] {
        assert_eq!(filter::check(&s, txt), Verdict::Keep, "dropped: {txt}");
    }
}

#[test]
fn an_artifact_is_dropped_however_it_is_punctuated() {
    let s = speech(1_000);
    for txt in [
        "Thank you.",
        "thank you",
        "  THANKS   for  watching  ",
        "[BLANK_AUDIO]",
        "(silence)",
        "Bye.",
        "Mm-hmm.",
    ] {
        assert_eq!(filter::check(&s, txt), Verdict::Artifact, "kept: {txt}");
    }
}

#[test]
fn a_loop_is_dropped_even_though_the_sentences_differ() {
    // whisper repeats with wording drift, so matching identical sentences
    // alone misses it. Three sentences drawn from two spellings is the shape.
    let s = speech(3_000);
    for txt in [
        "Thank you. Thank you. Thanks for watching!",
        "Maybe. Perhaps. Maybe.",
        "Bye. Bye bye. Bye.",
    ] {
        assert_eq!(filter::check(&s, txt), Verdict::Artifact, "kept: {txt}");
    }
}

#[test]
fn distinct_sentences_are_not_a_loop() {
    // The rule is about how many *distinct* sentences there are, not whether
    // one of them repeats. Real speech does repeat.
    let s = speech(3_000);
    for txt in [
        "I think. I think. Maybe. Perhaps.",
        "Ship it. Then call Maya. Finally, write the summary.",
        "First commit. Second commit. Then push.",
    ] {
        assert_eq!(filter::check(&s, txt), Verdict::Keep, "dropped: {txt}");
    }
}

#[test]
fn silence_is_reported_as_silence_not_as_an_artifact() {
    let quiet = vec![0_i16; 16_000];
    assert_eq!(filter::check(&quiet, "Thank you."), Verdict::Silent);
    assert_eq!(filter::check(&[], "anything"), Verdict::Silent);
}

#[test]
fn a_murmur_below_the_floor_is_still_silence() {
    let murmur = vec![200_i16; 16_000];
    assert_eq!(filter::check(&murmur, "Thank you."), Verdict::Silent);
}

#[test]
fn a_speech_shaped_peak_clears_the_floor() {
    // 800/32768 is ~0.024, just over SILENCE_FLOOR. Is the floor too tight?
    let just_over = vec![800_i16; 16_000];
    let just_under = vec![650_i16; 16_000];

    assert_eq!(
        filter::check(&just_over, "hello"),
        Verdict::Keep,
        "the floor should not eat quiet speech"
    );
    assert_eq!(filter::check(&just_under, "hello"), Verdict::Silent);
}

#[test]
fn the_cap_fires_at_sixty_seconds() {
    assert_eq!(filter::check(&speech(59_000), "hello"), Verdict::Keep);
    assert_eq!(filter::check(&speech(60_000), "hello"), Verdict::Keep);
    assert_eq!(filter::check(&speech(60_001), "hello"), Verdict::TooLong);
}

#[test]
fn the_cap_is_checked_before_anything_expensive() {
    // A runaway clip must not reach the artifact pass.
    let loud_and_huge = vec![i16::MAX; 16_000 * 61];
    assert_eq!(
        filter::check(&loud_and_huge, "Thank you."),
        Verdict::TooLong
    );
}

#[test]
fn words_with_no_speech_are_empty_not_silent() {
    // Symbols only: nothing to say, rather than noise to complain about.
    let s = speech(2_000);
    for txt in ["", "   ", "...", "♪", "♪ ♪", "()"] {
        assert_eq!(filter::check(&s, txt), Verdict::Empty, "got: {txt:?}");
    }
}

#[test]
fn the_bracket_tags_whisper_invents_are_dropped() {
    // [MUSIC] normalises to "music", [BLANK_AUDIO] to "blank audio" — the
    // underscore has to become a space or neither ever matches.
    let s = speech(1_000);
    for txt in [
        "[MUSIC]",
        "[BLANK_AUDIO]",
        "(upbeat music)",
        "Thanks for watching!",
    ] {
        assert_eq!(filter::check(&s, txt), Verdict::Artifact, "kept: {txt}");
    }
}

#[test]
fn every_drop_has_something_to_say() {
    assert_eq!(filter::verdict_msg(Verdict::Keep), "");
    for v in [
        Verdict::Silent,
        Verdict::Artifact,
        Verdict::TooLong,
        Verdict::Empty,
    ] {
        assert!(!filter::verdict_msg(v).is_empty(), "{v:?} says nothing");
    }
}

// --- the split -----------------------------------------------------------

#[test]
fn the_pre_gate_needs_no_transcript() {
    // The whole point of the split: a runaway clip is refused before it costs
    // a model load, so these cannot look at text at all.
    assert_eq!(filter::precheck(&speech(61_000)), Some(Verdict::TooLong));
    assert_eq!(filter::precheck(&vec![0_i16; 16_000]), Some(Verdict::Silent));
    assert_eq!(filter::precheck(&speech(1_000)), None);
}

#[test]
fn the_post_gate_needs_no_audio() {
    assert_eq!(filter::postcheck("Thank you."), Some(Verdict::Artifact));
    assert_eq!(filter::postcheck("   "), Some(Verdict::Empty));
    assert_eq!(filter::postcheck("ship it"), None);
}

#[test]
fn the_split_does_not_change_what_gets_dropped() {
    // `check` is the composition of the two halves, so these are the whole
    // truth table: every case that must survive does, every case that must
    // not is named rather than left to the reader.
    let loud = speech(2_000);
    let quiet = vec![0_i16; 16_000];
    let long = vec![8_000_i16; 16_000 * 61];

    for (samples, text, want) in [
        (&loud, "ship it", Verdict::Keep),
        (&loud, "Thank you.", Verdict::Artifact),
        (&loud, "", Verdict::Empty),
        (&quiet, "ship it", Verdict::Silent),
        (&long, "ship it", Verdict::TooLong),
        (&loud, "delete the database", Verdict::Keep),
    ] {
        assert_eq!(filter::check(samples, text), want, "{text:?}");
    }
}

#[test]
fn the_size_gate_wins_over_the_silence_gate() {
    // A clip that is both silent and too long is reported as too long, so the
    // message names the mistake the user can act on.
    assert_eq!(
        filter::check(&vec![0; 16_000 * 61], "ship it"),
        Verdict::TooLong
    );
}
