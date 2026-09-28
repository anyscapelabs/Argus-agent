# Frontend markdown + tag rendering — fix plan

> Four leaks, all reproduced against the current code. One is a real
> regression; two are long-standing; one from the old findings document did not
> reproduce and is dropped.

## The short version

Every leak in this document has the same shape: **a piece of state that is
allowed to leak across a boundary it does not own.**

- A tag attribute carries a newline, and the tokenizer is line-scoped.
- An unterminated tag flips a flag that suppresses markdown for the rest of the
  message.
- A renderer is handed a string where its siblings are handed parsed nodes.
- A turn timer is cleared by one event but driven by another.

None of these need new machinery. Three are one-line changes.

---

## What is actually happening

All four were verified by reading the code and by porting the relevant
functions to a probe and running them. No claim below is taken on faith.

### 1. A multi-line command breaks its own tag — **real, this is the regression**

`esc_attr` (`src-tauri/src/sessions/chat.rs:565`):

```rust
fn esc_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}
```

It escapes `&`, `"` and `<`. It does **not** escape `>` or newlines. A command
containing a newline therefore emits a raw newline inside an attribute:

```
<terminal id="a1" command="for f in *; do
  echo $f
done" status="ok" duration_ms="0">out</terminal>
```

The frontend tokenizer is quote-aware (`findTagEnd`, `agentXml.ts:266`) but
**line-scoped**: `tokenize` rejects any tag whose body contains a newline
(`agentXml.ts:321`). So the whole `<terminal>` block fails to parse — the
`status` and `duration_ms` attributes are lost with it, and the command spills
into the chat as prose.

Probe result — which shapes actually break:

| command | tokenizes |
|---|---|
| `ls -la` | yes |
| `grep foo x > out.txt` | yes (quote-aware, `>` is safe) |
| `cd /tmp && ls` | yes |
| `test 1 -lt 2 && echo y` | yes |
| **multi-line (loop, heredoc, `&&` chain)** | **no — leaks** |

Only multi-line breaks. This is the single highest-value fix here: a shell
loop or heredoc is completely ordinary agent output.

The `>` omission is latent rather than active — quote-awareness is currently
protecting it. It is fixed in the same change because it is the same missing
invariant.

### 2. An unterminated tag suppresses markdown for the whole message — **real**

`normalizeMd` (`agentXml.ts:848-866`) sets `inTag = true` when a line contains a
`<` with no closing `>`, and while `inTag` is set every subsequent line is
pushed through **untouched** (`agentXml.ts:849`).

Probe:

```
== baseline ==
   [md-ok] before
   [md-ok] # Heading
   [md-ok] **bold**
== unterminated tag ==
   [md-ok] before
   <terminal cmd="ls        <- inTag latches
   # Heading                <- raw
   **bold**                 <- raw
```

So one stray `<` in the middle of a long answer means every heading, bold run
and list after it renders as literal markdown. This matches the reported
`**Graceful shutdown…` symptom.

A `<` in prose does *not* trigger it — `tokenize` and `normalizeMd` agree that
a tag needs a name (`agentXml.ts:315`), and the probe confirms `x < y` is
handled correctly. It takes an unterminated *tag-like* sequence.

### 3. AlertBanner renders raw strings — **real, two-line fix**

`AlertBanner.tsx:19`:

```ts
.flatMap((s) => s.split(/(?<=[.!?])\s+/).filter(Boolean));
```

then renders each item as a React text child. Every other block renderer routes
through `renderInline` (`AgentBubble.tsx:120,136,153,165,178,265`). AlertBanner
is the only one that does not — so `<code>`, `<bold>`, and `<dim>` inside a
warning show as literal angle brackets.

The `split(/(?<=[.!?])\s+/)` is a second, smaller problem: it breaks a bullet at
*every* sentence boundary, so a three-sentence caveat becomes three bullets
regardless of whether the author wrote three bullets.

### 4. Stale "Working…" after a sub-agent finishes — **real**

`sessions.ts:381`:

```ts
if (ev.type === "turn_end") {
  this.clearTurn(sessionId);
```

A sub-agent's completion arrives as a `refresh` event, which reloads messages
and sessions but **does not clear the turn** (`sessions.ts:372-377`). The header
reads `turns[agentId]`, which only `turn_end` clears. A sub-agent's events go on
its own bus with no replay, so a missed `turn_end` leaves the spinner up
indefinitely while the database correctly says finished.

---

## What did not reproduce

**Zero-width spaces.** The old findings document claimed 409 of 560 messages
contained one and that this broke headings. Both numbers were wrong. The `409`
came from an empty regex matching every message; the real count is **2**, and
**no tag in the database carries an invisible character**.

The claim that a ZWSP before `#` leaves the heading literal is technically true
in isolation but has no supporting evidence in this repository's data. Separately,
the GLM research established that GLM's `<tool_call>` token is **pure ASCII** —
verified codepoint by codepoint — so a ZWSP there is model corruption, not
protocol.

`strip_invisibles` already runs in Rust (`tools/mod.rs:714`) before the frontend
sees anything. A belt-and-braces call in `parse()` is cheap and worth having,
but it is **not** a fix for a bug we have evidence of. It is filed as
hardening, not as a leak fix.

---

## What Karpathy actually says

Researched across all 13 posts on his Bear blog, karpathy.ai,
karpathy.github.io, the `autoresearch` / `nanochat` / `llm-council` /
`hn-time-capsule` / `rendergit` repos, the YC keynote description, and the
Dwarkesh transcript.

**Up front, because it changes what follows: he has written nothing about tag
handling.** No occurrence of "XML" in any Karpathy source. No writing on
zero-width characters. No AGENTS.md-specific material. A post about invisible
characters may exist as an X post that could not be reached — treat it as
unresolved, not refuted. **Nothing below should be cited as Karpathy on
protocols, because he has not written about protocols.**

What he has written is directly relevant to *why* these bugs are in the shape
they are.

### The loop belongs to the harness, and the harness owns the failure

> "Claude Code (CC) emerged as the first convincing demonstration of what an LLM
> Agent looks like — something that in a loopy way strings together tool use and
> reasoning for extended problem solving."
> — [2025 LLM Year in Review](https://karpathy.bearblog.dev/year-in-review-2025/)

> "LLM apps will organize, finetune and actually animate teams of them into
> deployed professionals in specific verticals **by supplying private data,
> sensors and actuators and feedback loops.**"
> — same

Applied here: none of the four leaks is a model failure. GLM produced a
multi-line command; a heredoc is normal and correct. **Argus** failed to encode
it. Because the loop is the harness's, the harness is what has to be right — and
there is no model-side change that would prevent a newline inside a string
attribute.

### Verifiability is the bottleneck — and this is the useful one

> "If a task/job is verifiable, then it is optimizable directly or via
> reinforcement learning... **It's about to what extent an AI can 'practice'
> something.** The environment has to be: **resettable** (you can start a new
> attempt), **efficient** (a lot attempts can be made) and **rewardable** (there
> is some automated process to reward any specific attempt that was made)."
> — [Verifiability](https://karpathy.bearblog.dev/verifiability/)

> "Software 1.0 easily automates what you can specify. **Software 2.0 easily
> automates what you can verify.**"
> — same

This is the principle the whole plan is built on. Every fix here converts an
unverifiable guess into a check:

| before | after |
|---|---|
| does this tag parse? (heuristic) | can this string be a valid attribute value? (decidable) |
| is markdown applied? (flag latched, invisible) | is the state scoped to a line? (structural) |
| does the banner look right? (eyeball) | does every block renderer use the same path? (structural) |
| is the turn over? (one event says so) | is the agent's reported state finished? (the actual question) |

`autoresearch` is the same idea in a loop: *"It modifies the code, trains for 5
minutes, checks if the result improved, keeps or discards, and repeats."* The
human's contribution is `program.md` — the context, not the code. Argus's
contribution is `tools::section()` and the renderer, not the weights.

### Tool failures are stale-knowledge failures; fix the surface, not the model

> "**Claude kept hallucinating deprecated APIs, model names, and input/output
> conventions** that have all changed recently"
> — [Vibe coding MenuGen](https://karpathy.bearblog.dev/vibe-coding-menugen/)

> "All of these services could become more LLM friendly... **Don't talk to a
> developer. Don't ask a developer to visit, look, or click. Instruct and
> empower their LLM.**"
> — same

Directly applicable to the protocol plan: when GLM emits XML Argus did not
expect, the right response is to teach the environment the real dialect, not to
correct the model.

### The reading that argues against a fix

> "**You can outsource your thinking, but you can't outsource your
> understanding.**"
> — [Sequoia Ascent 2026](https://karpathy.bearblog.dev/sequoia-ascent-2026/)
> (LLM-generated transcript of his talk; he reviewed it — treat as his ideas,
> not verbatim speech)

> "LLMs = 'people spirits'... simultaneously superhuman in some ways, but also
> fallible in many others." — [Software Is Changing (Again)](https://www.youtube.com/watch?v=LCEmiRjPEtQ), 14:39

The frontend leaks will recur. The goal is a renderer that fails **loudly at
one line** rather than silently degrading for the rest of a message. Every fix
below is chosen for that property.

---

## The fixes

Ordered by value per line changed. Steps 1 and 2 are the whole visible bug.

### 1. Escape what a value can legally contain — `chat.rs:565`

```rust
fn esc_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
}
```

**Both halves are required.** The newline escape makes the tag parse again. The
`>` escape restores the invariant that the tokenizer's quote-awareness depends
on — it is currently unheld.

Then teach `decodeEntities` (`agentXml.ts:180`) the numeric forms, or the
command displays as literal `&#10;`:

```ts
str.replace(/&#(\d+);/g, (_, n) => String.fromCodePoint(Number(n)))
```

**Test:** a terminal block with a multi-line command round-trips to the exact
original command string. Assert on the *reconstructed* command, not on the
absence of a crash — that is what "verifiable" means here.

### 2. Scope tag state to a line — `agentXml.ts:848`

`normalizeMd` already refuses to treat a multi-line construct as a tag
(`agentXml.ts:321`). `normalizeMd` must agree with it.

Replace the latching `inTag` flag with the same rule the tokenizer uses: if the
line has no tag end, treat **that line only** as raw, and do not suppress
markdown on the following lines.

```ts
if (lt !== -1 && findTagEnd(line, lt + 1) === -1) {
  out.push(line);
  k++;
  continue;                 // one line, not the rest of the message
}
```

The `inTag` variable and its block both go. Markdown inside a genuinely
multi-line construct is still protected by the `depth > 0` branch above it —
that path uses a real, closed tag, not a dangling one.

**Test:** an unterminated `<terminal` mid-message leaves the following heading
and bold run normalized. This is the `**Graceful shutdown…` case.

### 3. Render AlertBanner like every other banner — `AlertBanner.tsx`

```ts
import { renderInline } from "./InlineText";
```

and render `renderInline([{ kind: "text", value: it }])` per item, matching
`AgentBubble.tsx:136`.

While there: drop `split(/(?<=[.!?])\s+/)`. Bullets are lines. A sentence is
not a bullet, and the model was told to write short sentences.

**Test:** `<code>foo</code>` inside a warning renders as code, not as text.

### 4. Clear the turn from the agent's actual state — `sessions.ts:381`

The footer already reads database state. Make the header agree: on `refresh`,
after `loadAgents`, clear the turn for any agent not in a running state.

```ts
if (ev.type === "refresh") {
  void this.loadMsgs(sessionId);
  void this.loadAgents(sessionId).then(() => {
    for (const a of Object.values(this.state.agentRuns)) {
      if (a.state !== "running") this.clearTurn(a.sessionId);
    }
  });
  void this.loadSessions();
  return;
}
```

**Test:** a session whose sub-agent reports finished has no active turn after
`refresh`.

### 5. Hardening — strip invisibles in `parse()`

```ts
export function parse(buf: string): XmlTree {
  return buildTree(tokenize(normalizeMd(buf.replace(INVISIBLE_RE, ""))));
}
```

Filed as **hardening, not a fix** — we have no evidence of a ZWSP reaching the
frontend. Cheap, and it makes streamed text and stored text agree. Do not
describe this as closing a reported bug.

---

## Tests

Frontend has an existing suite (10 tests, passing). New in `src/lib/agentXml.test.ts`:

- a multi-line terminal command survives the round trip intact
- `>` in a command does not close the tag
- an unterminated tag does not suppress markdown on later lines
- `&`, `<`, `>`, newline all round-trip through `esc_attr`/`decodeEntities`
- a warning block renders inline markup rather than literal tags

Backend, in `src-tauri/tests/`: a `terminal_block` test asserting a
multi-line command produces a single-line tag with a numeric-escaped attribute.
Note the backend test must assert the *escaped* form, since the frontend owns
the decode.

## What this does not fix

- **The protocol leaks** in `docs/developer-guide/tool-call-protocol.md` are a
  separate document and a separate cause. This one is about rendering.
- **The stale turn** is a symptom of a wider gap: a sub-agent's event bus has
  no replay, so any missed event is permanent. Step 4 makes the header
  self-correcting; it does not make the bus reliable.
- **Leaks will recur.** These fixes make each one fail at a single line instead
  of corrupting the rest of the message. That is the achievable goal.

## Risks

| risk | note |
|---|---|
| `&#10;` reaches the model in history | the tool block is stripped from history already (`prompt/mod.rs:415`); verify after the change |
| `> → &gt;` changes existing rendering | only affects attribute values, which are not rendered as markup |
| step 2 could un-protect multi-line tag bodies | the `depth > 0` branch still handles closed multi-line tags; the test in step 2 covers it |
| numeric entities double-decode | `decodeEntities` runs once, on a freshly sliced string — no re-entrancy |

## Revert

Four independent commits, each with a test. Any one reverts alone.
