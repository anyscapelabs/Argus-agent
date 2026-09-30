// Microphone capture for dictation.
//
// The format half of this file is pure and tested: the resampler, the WAV
// header and the level meter. The device half below cannot be tested here --
// there is no AudioContext in the test runner -- so it is deliberately thin and
// does nothing that is not one of those functions.

// whisper's mel transform assumes 16 kHz and does not check. A 48 kHz clip
// decodes to confident garbage rather than an error, so the rate is fixed here
// instead of being whatever the device felt like offering.
export const TARGET_RATE = 16000;

// Unbounded capture is unbounded memory, and there is nothing to size it
// against. Matches the backend's own cap; whichever fires first stops it.
export const MAX_SECS = 60;

// WebKitGTK honours the constructor but Chrome and Safari do not always, so
// this is checked rather than assumed.
const WORKLET = `
class ArgusRecorder extends AudioWorkletProcessor {
  process(inputs) {
    const ch = inputs[0] && inputs[0][0];
    if (ch && ch.length) this.port.postMessage(new Float32Array(ch));
    return true;
  }
}
registerProcessor("argus-recorder", ArgusRecorder);
`;

// Taps for the pre-filter. Fifteen is enough to keep a 48 kHz feed from
// folding into the speech band without making a minute-long take cost more
// than the decode that follows it.
const TAPS = 15;

/**
 * Hamming-windowed sinc low-pass, run before the decimation.
 *
 * Without it, every frequency above the new Nyquist folds back down into the
 * band whisper actually listens to, and it hears the result as words. That is
 * the same class of failure as the wrong sample rate: the decode succeeds and
 * confidently returns something wrong.
 */
function lowpass(input: Float32Array, cutoff: number): Float32Array {
  const out = new Float32Array(input.length);
  const mid = (TAPS - 1) / 2;
  const taps = new Float32Array(TAPS);
  let sum = 0;

  for (let i = 0; i < TAPS; i++) {
    const x = i - mid;
    const sinc = x === 0 ? 2 * cutoff : Math.sin(2 * Math.PI * cutoff * x) / (Math.PI * x);
    const window = 0.54 - 0.46 * Math.cos((2 * Math.PI * i) / (TAPS - 1));

    taps[i] = sinc * window;
    sum += taps[i];
  }

  for (let i = 0; i < TAPS; i++) {
    taps[i] /= sum;
  }

  for (let i = 0; i < input.length; i++) {
    let acc = 0;

    for (let j = 0; j < TAPS; j++) {
      const at = i + j - mid;
      if (at >= 0 && at < input.length) acc += input[at] * taps[j];
    }

    out[i] = acc;
  }

  return out;
}

export function resample(input: Float32Array, from: number, to: number): Float32Array {
  if (from === to || input.length === 0) {
    return input;
  }

  const cutoff = to / (2 * from);
  const src = cutoff >= 0.5 ? input : lowpass(input, cutoff);
  const ratio = from / to;
  const out = new Float32Array(Math.round(src.length / ratio));

  for (let i = 0; i < out.length; i++) {
    const pos = i * ratio;
    const i0 = Math.floor(pos);
    const i1 = Math.min(i0 + 1, src.length - 1);
    const t = pos - i0;
    out[i] = src[i0] * (1 - t) + src[i1] * t;
  }

  return out;
}

export function bytesToBase64(bytes: Uint8Array): string {
  // One spread of two million arguments blows the stack, so it is chunked.
  let bin = "";
  const CHUNK = 0x8000;

  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }

  return btoa(bin);
}

export function encodeWav(samples: Float32Array, sampleRate: number): string {
  const dataBytes = samples.length * 2;
  const buf = new ArrayBuffer(44 + dataBytes);
  const view = new DataView(buf);

  const magic = (at: number, text: string) => {
    for (let i = 0; i < text.length; i++) view.setUint8(at + i, text.charCodeAt(i));
  };

  magic(0, "RIFF");
  view.setUint32(4, 36 + dataBytes, true);
  magic(8, "WAVE");
  magic(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true); // PCM
  view.setUint16(22, 1, true); // mono
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  magic(36, "data");
  view.setUint32(40, dataBytes, true);

  for (let i = 0; i < samples.length; i++) {
    const s = Math.max(-1, Math.min(1, samples[i]));
    // Asymmetric on purpose: full scale is -32768 to 32767, not -32767 to
    // 32767, and dividing the wrong end pushes the loudest sample past 1.0.
    view.setInt16(44 + i * 2, s < 0 ? s * 0x8000 : s * 0x7fff, true);
  }

  return bytesToBase64(new Uint8Array(buf));
}

export function levelOf(samples: Float32Array): number {
  if (samples.length === 0) {
    return 0;
  }

  let sum = 0;
  for (let i = 0; i < samples.length; i++) sum += samples[i] * samples[i];

  return Math.sqrt(sum / samples.length);
}

export type Take = { pcm: Float32Array; secs: number };

/**
 * Owns the microphone for the length of one utterance.
 *
 * The AudioContext is created per take rather than once for the app. That is
 * the opposite of the usual advice, and it is deliberate: a context that
 * outlives a cancelled recording holds the device open, and WebKit's
 * concurrent-context limit is a worse failure than a few milliseconds of setup.
 */
export class Recorder {
  private ctx: AudioContext | null = null;
  private stream: MediaStream | null = null;
  private node: AudioWorkletNode | null = null;
  private chunks: Float32Array[] = [];
  private rate = TARGET_RATE;

  async start(onLevel: (n: number) => void): Promise<void> {
    // Plain audio, not a constrained profile. PipeWire rejects an
    // over-constrained request outright and the user gets no mic at all.
    this.stream = await navigator.mediaDevices.getUserMedia({ audio: true });

    this.ctx = new AudioContext();
    await this.ctx.audioWorklet.addModule(
      URL.createObjectURL(new Blob([WORKLET], { type: "application/javascript" })),
    );

    this.rate = this.ctx.sampleRate;
    this.chunks = [];

    this.node = new AudioWorkletNode(this.ctx, "argus-recorder");
    this.node.port.onmessage = (e: MessageEvent) => {
      const block = e.data as Float32Array;
      this.chunks.push(block);
      onLevel(levelOf(block));
    };

    this.ctx.createMediaStreamSource(this.stream).connect(this.node);

    // A context created while no gesture is being handled starts suspended,
    // which records pure silence and looks like a dead microphone.
    if (this.ctx.state === "suspended") {
      await this.ctx.resume();
    }
  }

  /** Hand back everything captured, resampled to what whisper expects. */
  take(): Take {
    const raw = concat(this.chunks);
    this.teardown();

    return {
      pcm: resample(raw, this.rate, TARGET_RATE),
      secs: raw.length / this.rate,
    };
  }

  cancel(): void {
    this.chunks = [];
    this.teardown();
  }

  private teardown() {
    this.node?.port.close();
    this.node?.disconnect();
    this.stream?.getTracks().forEach((t) => t.stop());
    void this.ctx?.close();

    this.node = null;
    this.stream = null;
    this.ctx = null;
  }
}

function concat(parts: Float32Array[]): Float32Array {
  let total = 0;
  for (const p of parts) total += p.length;

  const out = new Float32Array(total);
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }

  return out;
}