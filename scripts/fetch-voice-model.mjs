// Fetches the speech model that ships inside the app.
//
// Clicking the mic has to start recording, not start a download. The file is
// 57 MB, it only changes when someone deliberately cuts a new build, and a
// build-time fetch is the only place this is invisible to the user.
//
// It is fetched rather than committed: 57 MB of binary does not belong in the
// history of every clone, and the checksum below is what actually makes it
// trustworthy.
//
// The key here must match an entry in src-tauri/src/voice/download.rs. That
// file is the allowlist the running app uses; this one is the build-time side
// of the same list.

import { createHash } from "node:crypto";
import { createWriteStream } from "node:fs";
import { mkdir, readFile, rename, rm, stat } from "node:fs/promises";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const OUT = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "resources", "voice");

const WANTED = [
  { repo: "ggerganov/whisper.cpp", file: "ggml-base-q5_1.bin" },
  { repo: "ggml-org/whisper-vad", file: "ggml-silero-v6.2.0.bin" },
];

async function expectedSha(repo, file) {
  const res = await fetch(`https://huggingface.co/api/models/${repo}/tree/main`);

  if (!res.ok) {
    throw new Error(`huggingface returned ${res.status} listing ${repo}`);
  }

  const entries = await res.json();
  const hit = entries.find((e) => e.path === file && e.lfs);

  if (!hit) {
    throw new Error(`huggingface published no checksum for ${repo}/${file}`);
  }

  return hit.lfs.oid;
}

async function present(path) {
  return stat(path).then(() => true, () => false);
}

async function fetchOne({ repo, file }) {
  const dest = join(OUT, file);
  const part = `${dest}.part`;

  if (await present(dest)) {
    console.log(`  ${file} — already here`);
    return;
  }

  const want = await expectedSha(repo, file);
  const res = await fetch(`https://huggingface.co/${repo}/resolve/main/${file}`);

  if (!res.ok) {
    throw new Error(`huggingface returned ${res.status} for ${file}`);
  }

  await pipeline(Readable.fromWeb(res.body), createWriteStream(part));

  const bytes = await readFile(part);
  const got = createHash("sha256").update(bytes).digest("hex");

  if (got !== want) {
    await rm(part, { force: true });
    throw new Error(`${file} did not match its checksum — build aborted`);
  }

  // Rename last, so an interrupted build leaves a .part rather than a
  // truncated model that the next build would mistake for a good one.
  await rename(part, dest);
  console.log(`  ${file} — ${(bytes.byteLength / 1_048_576).toFixed(1)} MB`);
}

await mkdir(OUT, { recursive: true });

for (const item of WANTED) {
  await fetchOne(item);
}