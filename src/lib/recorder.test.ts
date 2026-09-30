import { describe, expect, it } from "bun:test";

import {
  bytesToBase64,
  encodeWav,
  levelOf,
  resample,
  TARGET_RATE,
} from "./recorder";
import { insertTranscript } from "./voiceText";

function f32(...values: number[]): Float32Array {
  return new Float32Array(values);
}

function headerOf(b64: string): DataView {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return new DataView(bytes.buffer);
}

describe("resample", () => {
  it("returns the input untouched when the rate already matches", () => {
    const input = f32(0, 0.5, -0.5);
    expect(resample(input, TARGET_RATE, TARGET_RATE)).toBe(input);
  });

  it("halves the sample count when halving the rate", () => {
    const input = new Float32Array(1000).fill(0.5);
    expect(resample(input, 32000, TARGET_RATE).length).toBe(500);
  });

  it("doubles the sample count when doubling the rate", () => {
    const input = new Float32Array(500).fill(0.5);
    expect(resample(input, TARGET_RATE, 32000).length).toBe(1000);
  });

  it("interpolates between neighbours rather than dropping them", () => {
    // A 3:2 ratio lands half way between samples on every other output
    // sample, which is the only way to see the interpolation at all. Halving
    // is exact by construction and would pass with nearest-neighbour.
    const input = new Float32Array(8);
    for (let i = 4; i < 8; i++) input[i] = 1;

    const out = resample(input, 3, 2);
    expect(out.length).toBe(5);
    expect(out[3]).toBeGreaterThan(0);
    expect(out[3]).toBeLessThan(1);
  });

  it("copes with an empty take", () => {
    expect(resample(new Float32Array(0), 48000, TARGET_RATE).length).toBe(0);
  });

  it("never reads past the end on the final sample", () => {
    const input = new Float32Array([0, 1, 0, 1]);
    for (const v of resample(input, 3, TARGET_RATE)) {
      expect(Number.isFinite(v)).toBe(true);
    }
  });

  it("rejects the frequencies that would alias into the speech band", () => {
    // A tone well above the 8 kHz that survives to whisper would come back as
    // audible words it is not. Low-passing before decimating is what stops it.
    const rate = 48000;
    const hz = 12000;
    const input = new Float32Array(rate / 10);
    for (let i = 0; i < input.length; i++) {
      input[i] = Math.sin((2 * Math.PI * hz * i) / rate);
    }

    const out = resample(input, rate, TARGET_RATE);

    // Ignore the filter's edges, which see fewer samples than taps.
    const middle = out.subarray(50, out.length - 50);
    let peak = 0;
    for (const v of middle) peak = Math.max(peak, Math.abs(v));

    expect(peak).toBeLessThan(0.05);
  });

  it("keeps a tone inside the speech band", () => {
    // The other half of the same test: a filter that flattens everything is
    // just as wrong as one that lets everything through.
    const rate = 48000;
    const hz = 1000;
    const input = new Float32Array(rate / 10);
    for (let i = 0; i < input.length; i++) {
      input[i] = Math.sin((2 * Math.PI * hz * i) / rate);
    }

    const out = resample(input, rate, TARGET_RATE);

    const middle = out.subarray(50, out.length - 50);
    let peak = 0;
    for (const v of middle) peak = Math.max(peak, Math.abs(v));

    expect(peak).toBeGreaterThan(0.8);
  });

  it("leaves a signal alone when no decimation is happening", () => {
    const input = new Float32Array([0, 1, 0, -1]);
    expect(resample(input, TARGET_RATE, TARGET_RATE)).toBe(input);
  });
});

describe("encodeWav", () => {
  it("writes a header the backend will accept", () => {
    const view = headerOf(encodeWav(f32(0, 0.5), TARGET_RATE));
    const tag = (at: number, len: number) =>
      String.fromCharCode(...new Uint8Array(view.buffer, at, len));

    expect(tag(0, 4)).toBe("RIFF");
    expect(tag(8, 4)).toBe("WAVE");
    expect(tag(12, 4)).toBe("fmt ");
    expect(tag(36, 4)).toBe("data");
    expect(view.getUint16(20, true)).toBe(1);
    expect(view.getUint16(22, true)).toBe(1);
    expect(view.getUint32(24, true)).toBe(TARGET_RATE);
    expect(view.getUint16(34, true)).toBe(16);
    expect(view.getUint32(40, true)).toBe(4);
    expect(view.getUint32(4, true)).toBe(36 + 4);
  });

  it("sizes the clip from the sample count", () => {
    const view = headerOf(encodeWav(new Float32Array(16000).fill(0.1), TARGET_RATE));
    expect(view.getUint32(40, true)).toBe(32000);
  });

  it("keeps full scale inside the range whisper expects", () => {
    // The asymmetry is the whole point: -1 has to reach -32768, not -32767,
    // or the loudest possible sample lands outside [-1, 1] after the backend
    // normalises it again.
    const view = headerOf(encodeWav(f32(1, -1, 2, -2), TARGET_RATE));
    expect(view.getInt16(44, true)).toBe(32767);
    expect(view.getInt16(46, true)).toBe(-32768);
    expect(view.getInt16(48, true)).toBe(32767);
    expect(view.getInt16(50, true)).toBe(-32768);
  });

  it("survives a clip with no samples", () => {
    const view = headerOf(encodeWav(new Float32Array(0), TARGET_RATE));
    expect(view.getUint32(40, true)).toBe(0);
  });
});

describe("bytesToBase64", () => {
  it("round-trips a small payload", () => {
    const bytes = new Uint8Array([0, 1, 127, 128, 255]);
    expect(bytesToBase64(bytes)).toBe(btoa(String.fromCharCode(...bytes)));
  });

  it("handles a payload larger than one chunk", () => {
    // One spread of 100k arguments throws; chunking is the whole reason it is
    // a separate function.
    const bytes = new Uint8Array(100_000).fill(7);
    expect(bytesToBase64(bytes)).toBe(btoa(String.fromCharCode(...bytes)));
  });
});

describe("levelOf", () => {
  it("is zero for silence and one for full scale", () => {
    expect(levelOf(new Float32Array(100))).toBe(0);
    expect(levelOf(new Float32Array(100).fill(1))).toBeCloseTo(1, 5);
  });

  it("reads quieter than the loudest sample", () => {
    // One spike in a second of quiet is not loud speech, and a meter driven by
    // the peak would sit pinned at the top through the whole utterance.
    const spike = new Float32Array(16000);
    spike[8000] = 1;
    expect(levelOf(spike)).toBeLessThan(0.05);
  });

  it("copes with an empty block", () => {
    expect(levelOf(new Float32Array(0))).toBe(0);
  });
});

describe("insertTranscript", () => {
  it("puts the text into an empty box with no leading space", () => {
    expect(insertTranscript("", 0, "hello")).toEqual({
      text: "hello",
      caret: 5,
    });
  });

  it("inserts at the caret, not the end", () => {
    // The user typed ahead while recording. Splicing at the end would move
    // their text; the caret is where they said the words go.
    expect(insertTranscript("hello world", 5, " there")).toEqual({
      text: "hello there world",
      caret: 11,
    });
  });

  it("does not fuse the transcript onto a half-typed word", () => {
    expect(insertTranscript("hello", 5, "world").text).toBe("hello world");
  });

  it("does not double the space when there already is one", () => {
    expect(insertTranscript("hello ", 6, "world").text).toBe("hello world");
  });

  it("keeps a leading space inside the transcript", () => {
    expect(insertTranscript("hi", 2, " there").text).toBe("hi there");
  });

  it("clamps a caret that no longer fits the text", () => {
    // The box changed while the recording ran. A caret past the end would
    // throw away whatever the user typed.
    expect(insertTranscript("hi", 99, " there").text).toBe("hi there");
    expect(insertTranscript("hi", -3, "there").text).toBe("therehi");
  });

  it("leaves the box alone when there was nothing said", () => {
    expect(insertTranscript("hi", 2, "")).toEqual({ text: "hi", caret: 2 });
    expect(insertTranscript("hi", 2, "   ")).toEqual({ text: "hi", caret: 2 });
  });
});