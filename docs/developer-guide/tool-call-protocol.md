# Tool-call protocol — migration plan

> One protocol is not the answer. The answer is to stop fighting the format the
> model was trained on, and stop persisting the text that leaks.

## The short version

Argus tells every model *"never write a tool call as text."* GLM's own chat
template tells it, in the same context window, *"output the function name and
arguments within the following XML format."* Both instructions are in the
prompt. The model follows the one it was fine-tuned on, and Argus sees XML
where it expected `tool_calls`.

We are not recovering from an error. We are decoding a format the model was
trained to produce, and we never asked for it.

The fix is three things, in order:

1. **Stop contradicting the template** for models that emit XML.
2. **Never persist a text-channel tool call that duplicates a native call.**
3. **Mark per model whether it speaks XML** — instead of one rule for all.

Everything else in this document is supporting detail.

---

## What is actually happening

Argus sends native tool specs and gets structured calls back. The gateway is
correct:

- `wire_tools()` builds the OpenAI `tools` array (`gateway/adapters/openai_compat.rs:36`)
- `anthropic.rs` builds the Anthropic tool block
- `StreamDone` carries `text` and `tool_calls` separately (`gateway/schema.rs:167`)

The leak is entirely in how the *text* channel is handled afterwards.

### GLM's XML is its training format, not a malfunction

`zai-org/GLM-4.5`, `4.6` and `4.7` all ship a `chat_template.jinja` containing
this, injected verbatim into the system prompt whenever `tools` is non-empty:

> For each function call, output the function name and arguments within the
> following XML format: `<tool_call>{function-name}<arg_key>{arg-key-1}</arg_key><arg_value>{arg-value-1}</arg_value></tool_call>`

Sources: `chat_template.jinja` on each model's HuggingFace repo (lines 16-22,
65-66). The format is also the SFT target — the template is the serializer, and
the model learned to emit it.

Every one of those tags is a **dedicated single token**:

| id | token |
|---|---|
| 151352 | `<tool_call>` |
| 151353 | `</tool_call>` |
| 151356 | `<arg_key>` |
| 151357 | `</arg_key>` |
| 151358 | `<arg_value>` |
| 151359 | `</arg_value>` |

From `tokenizer_config.json` → `added_tokens_decoder`, identical across 4.5 /
4.6 / 4.7. A "missing `<arg_value>` tag" is therefore **one dropped token**.

The `<tool_call>` token is pure ASCII (verified codepoint by codepoint: 11
chars, zero non-ASCII bytes). **The zero-width space is model-side corruption,
not protocol.** `strip_invisibles()` (`tools/mod.rs:714`) is correct and stays —
it is corruption recovery, not format handling.

### Argus's prompt is the contradiction

`tools::section()` (`tools/mod.rs:1936`) says:

> To run a tool, call it through the tool-calling API you were given. Never
> write a tool call as text: not an action tag, not a tool-call tag, never args
> as tag attributes, never a self-closed tag.

The provider's template says the opposite, in the same context window. Argus's
instruction fights the model's SFT format. This is the root cause.

### The four failing shapes are all known upstream bugs

Not speculation — each is a filed vLLM issue with the same shape as our
symptom:

| Argus symptom | Upstream |
|---|---|
| missing `<arg_value>` | [#49248](https://github.com/vllm-project/vllm/issues/49248) — 5-17% of calls under concurrency 6, **0% sequential**, reproduces at temperature 0 |
| missing `<arg_key>` (`namearg</arg_key>`) | [#47190](https://github.com/vllm-project/vllm/pull/47190) |
| well-formed XML parsing to zero calls | [#58315](https://github.com/vllm-project/vllm/issues/58315) — grammar accepts sub-token markers, streaming parser only recognises atomic IDs |
| `arguments: "{}"` poisoning history | [#49248](https://github.com/vllm-project/vllm/issues/49248) — "the broken `{}` call gets rendered back into history" |

The closest published analogue is [vLLM #42400](https://github.com/vllm-project/vllm/issues/42400):
Claude Code against a GLM backend, "The model's tool call could not be parsed
(retry also failed)." **Resolved server-side, not client-side.**

### Our own history confirms it

Aggregated over 560 messages in the local database (no message content read):

| Model | Leaking | Assistant msgs | Rate |
|---|---|---|---|
| `baseten/zai-org/GLM-5.3-Fast` | 4 | 100 | 4.0% |
| `baseten/zai-org/GLM-5.2-Fast` | 4 | 38 | 10.5% |
| `DeepSeek-V4-Pro` | 0 | 56 | 0% |
| `ling-3.0-flash` | 0 | 56 | 0% |
| `space-bunny-alpha` | 0 | 8 | 0% |

**The leak is GLM-specific. 158 messages from five other models, zero leaks.**

Two further facts, both decisive:

1. **6 of 8 leaking assistant messages also had native `tool_calls` recorded.**
   The model called the tool *correctly* and additionally emitted XML as text.
   The leak is **duplication**, not failure to use the channel.
2. **All 3 user-role leaks are tool results** (`tool_call_id=chatcmpl-tool-…`).
   The raw XML is being stored as a tool *result* and fed back into history.

That is a closed loop:

```
native call succeeds
  → same call ALSO written as text by GLM
    → normalize_actions() salvages it into <action tool="…">{json}</action>
      → build_executions() skips it as a duplicate …
        … but the text is still in base_text and still gets persisted
          → stored as a tool RESULT
            → fed back into history
              → model imitates it harder next turn
```

The loop explains the `<String, Value>` leak — a Rust type name that reached
chat text. The model was echoing Argus's own internal `<action tool="…">{json}</action>`
representation back at us, because that form had been fed to it as a tool result.

**This also kills the premise of a "protocol all models understand."** Non-GLM
models leak 0%. Standardising them onto a new convention would re-educate working
models for zero benefit.

---

## The target

Per model, one of two styles. Argus declares which, in its own prompt, matching
what the provider's template already said.

| style | who | text channel |
|---|---|---|
| `native` | default — everything not known to emit XML | native `tool_calls`; text-channel tool syntax is **stripped**, never executed |
| `glm-xml` | GLM-4.5/4.6/4.7/5.x, and any model whose template says so | XML decoded deliberately by one parser; native calls still honoured |

Both styles converge on the same internal representation and the same
`<action tool="…">` block. The difference is only whether the text channel is
allowed to *produce* executions.

### Literal grammar for `glm-xml`

```xml
<tool_call>terminal
<arg_key>command</arg_key>
<arg_value>ls -la</arg_value>
</tool_call>
```

and the well-formed single-line form GLM-4.7 emits:

```xml
<tool_call>terminal<arg_key>command</arg_key><arg_value>ls -la</arg_value></tool_call>
```

A call a model may plausibly emit that must still be caught — the
[#49248](https://github.com/vllm-project/vllm/issues/49248) shape, missing
opening value tag:

```xml
<tool_call>terminal<arg_key>command</arg_key>ls -la</arg_value></tool_call>
```

Argus must run that as `terminal` with `command = "ls -la"`, and must **not**
emit `arguments: "{}"`.

### Non-negotiable

- **No new protocol is invented.** We adopt what the model already emits.
- **`arguments: "{}"` is never executed.** A call that parses to empty args when
  the model clearly wrote some is dropped, per [#49248](https://github.com/vllm-project/vllm/issues/49248).
- **The salvage layer never writes to the result channel.** It produces
  executions; it is never stored as a tool result.

---

## Prompt changes

`tools::section()` (`tools/mod.rs:1936`) becomes style-aware. The contradiction
goes away; the rule becomes *agree with the template* rather than *forbid the
template*.

For `native` models, the current text is nearly right — but the absolute
"never write a tool call as text" is demoted to a description of what the
channel is, and a hard strip is enforced in code rather than by asking the model
politely:

> ### Calling a tool
>
> Tools are called through the tool-calling API you were given. Your reply ends
> with either a tool call, or the final answer followed by `<final/>` on its own
> last line. Nothing else closes a turn.
>
> Write prose in the text channel. Anything that looks like a tool call written
> as text is discarded before it reaches you again, so a call written there is a
> step you lost.

For `glm-xml` models — the important half:

> ### Calling a tool
>
> The tool API you were given is already described above, and you should call
> tools through it.
>
> If you write a call as text, use exactly the format that description gives
> you: `<tool_call>name<arg_key>key</arg_key><arg_value>value</arg_value></tool_call>`.
> Keep each tag on the same line as its value, never nest a value inside a key,
> and never repeat a key.
>
> Write a key only for an argument you are actually passing. An argument you
> are not passing is left out — it is not written as an empty key.

That last line targets `<arg_key></arg_key>` (7 occurrences in our history) and
`<arg_key><arg_value>` (2) — the empty-value shapes that can silently become an
empty command.

**Delete** the "never a self-closed tag / never args as tag attributes" clause.
It is aimed at the old `<action>` dialect, is not something GLM produces, and
spends prompt budget on a shape we no longer parse.

---

## Backend changes

Each step is independently shippable. **Step 1 is the whole fix**; the rest
reduce rate and cost.

### 1. Never persist a text-channel duplicate — `tools/mod.rs:372`

`build_executions()` already skips a text action that duplicates a native call
(`tools/mod.rs:390-395`). The skip only affects *execution*. The raw XML is
still in `base_text`, still rendered, still persisted.

Change the return to carry the offsets of consumed text-channel spans, and
remove them from the text before it is stored or shown:

```rust
pub struct Executions {
    pub execs: Vec<ToolExecution>,
    /// Byte ranges of text that produced an execution. Everything here is
    /// already represented by `execs`; keeping it is how the model learns to
    /// imitate its own tool syntax.
    pub consumed: Vec<(usize, usize)>,
}
```

Caller change at `chat.rs:799` — strip the consumed ranges from `base_text`
before it is handed to `render_actions` and stored.

**This severs the feedback loop.** The 6-of-8 case is exactly the leak that
made leaks compound.

### 2. Never store a salvaged call as a tool result — `chat.rs:799`

Same root, different path. `prompt/mod.rs:415` already strips actions from
history via `strip_actions()`. Confirm the tool-result path does the same for
salvaged calls, and that `<String, Value>` can never reach a result.

### 3. Per-model `tool_call_style` — `gateway/mod.rs:340`

`models.capabilities` is already a JSON blob (`gateway/schema.rs:21`), written
from models.dev at `gateway/mod.rs:340`. No migration needed — add a key:

```json
{ "tools": true, "tool_call_style": "glm-xml" }
```

Default `"native"` when absent. Seed `"glm-xml"` for the GLM family from
models.dev's `family` field; do **not** hand-maintain a list.

### 4. Style-aware `section()` — `tools/mod.rs:1936`

`section(web)` becomes `section(web, style)`. The system prompt is assembled in
`prompt/mod.rs:120`; the style comes from the session's model capabilities.

### 5. `native` models: strip, do not execute — `tools/mod.rs:1032`

For `native`-style models, any text-channel tool syntax is removed from the
prose and **not** run. `strip_protocol()` already recognises the tags; extend
the call to cover the GLM wrapper for this style, and log a counter so we can
see whether any non-GLM model ever needs it.

### 6. Guard the `arguments: "{}"` case — `tools/mod.rs:386`

Per [#49248](https://github.com/vllm-project/vllm/issues/49248): when a
salvaged call has a non-empty tool name but parses to zero arguments, and the
raw text plainly contained argument content, **drop the call and return an
error string to the model** rather than executing an empty one. Do not render it
into history.

---

## What survives of the salvage layer

The existing recovery work is **correct and stays**. It was written against
real malformed output and matches the upstream bug reports. After this migration:

| case | fate |
|---|---|
| missing `<arg_value>` | **kept** — real bug [#49248](https://github.com/vllm-project/vllm/issues/49248) |
| missing `<arg_key>`, glued name | **kept** — [#47190](https://github.com/vllm-project/vllm/pull/47190) |
| zero-width space in tag | **kept** — corruption recovery, tokens are pure ASCII |
| wrapperless call | **kept** — [#47190](https://github.com/vllm-project/vllm/pull/47190) |
| multi-line / single-line variants | **kept** — GLM 4.5 and 4.7 differ, both are real |
| `<action>` dialect salvage | **unreachable** — the model is told not to write it, and the template says XML. Dead after this lands. |

**Nothing is deleted in this migration.** The claim that the salvage layer
becomes unnecessary is wrong: it handles server-side parser bugs we cannot fix
from the client, and every case in it corresponds to a filed upstream issue.
What changes is that it stops *feeding itself* — which is the actual defect.

The `<action>` dialect is the one genuinely dead path, and it is removed by
step 1, not by deleting code.

---

## Frontend changes

Minimal, and mostly already in place:

- `src/lib/agentXml.ts` — `strip_invisibles` is applied in Rust before render;
  add the same at `parse()` so streamed text matches the stored text.
- AlertBanner: pass "Things to note" items through `renderInline`. It prints
  raw strings today, so `<code>` and `<bold>` show literally. Not a tool-call
  bug, but the same "unstructured text reaching the user" class.

## Tests

`src-tauri/tests/` only. Existing `tools_test.rs` (46) must stay green — the
salvage cases are kept, so nothing there changes.

New in `tools_test.rs`:
- a native call plus an identical text call persists the text **nowhere**
- a text call with no native twin still executes (`glm-xml`)
- `native` style: text tool syntax is stripped, never executed
- a call that parses to `{}` is dropped, not executed, not rendered
- empty `<arg_key></arg_key>` does not become an empty command

New in `library_dup_test.rs` or a new `protocol_test.rs`:
- round-trip: model result → stored → next prompt contains no `arg_key`

## Risks

| risk | mitigation |
|---|---|
| GLM-5.x template differs from 4.5-4.7 | **verify before implementing step 3** — see below |
| removing the "never write as text" line lets other models drift | step 5 strips for `native` models, so drift is caught, not obeyed |
| step 1 loses a call that text-only parsing was rescuing | only spans that produced an **executed** call are removed |
| provider has no `--tool-call-parser` and returns text only | `glm-xml` still works — it is the text channel by definition |
| `tool_choice: "required"` tempting as a fix | **do not.** [#47504](https://github.com/vllm-project/vllm/issues/47504): GLM repeats the JSON until context exhaustion |

## Revert

Every step is independent. Step 1 is one function's return type. Reverting is
reverting a commit.

---

## The one thing to verify first

Our leaking models are **`GLM-5.3-Fast` and `GLM-5.2-Fast`**. All primary-source
research here covers **4.5 / 4.6 / 4.7**. The dialect is very likely stable
across the range, but the entire diagnosis rests on the template's contents.

Before implementing step 3, confirm `chat_template.jinja` for the GLM-5.x
weights and check that it still injects the same `arg_key`/`arg_value`
instruction. If GLM-5.x moved to a different dialect, step 3's seed list and
step 4's prompt text both change — steps 1, 2, 5 and 6 are unaffected either
way, because they do not depend on which dialect it is.

**Steps 1 and 2 are safe to ship today.** They fix the feedback loop on
evidence we already hold, and they do not depend on the dialect question.

---

## Sources

- [GLM-4.5 chat_template.jinja](https://huggingface.co/zai-org/GLM-4.5/raw/main/chat_template.jinja) — the injected tool instruction (lines 16-22, 65-66)
- [GLM-4.5 tokenizer_config.json](https://huggingface.co/zai-org/GLM-4.5/raw/main/tokenizer_config.json) — `added_tokens_decoder` 151350-151359
- [GLM-4.5 model card](https://huggingface.co/zai-org/GLM-4.5) — "Both support tool calling. Please use OpenAI-style tool description format"
- [vLLM #49248](https://github.com/vllm-project/vllm/issues/49248) / [PR #49249](https://github.com/vllm-project/vllm/pull/49249) — missing `<arg_value>`, measured 5-17%
- [vLLM PR #47190](https://github.com/vllm-project/vllm/pull/47190) — missing `<arg_key>`
- [vLLM #58315](https://github.com/vllm-project/vllm/issues/58315) / [PR #58847](https://github.com/vllm-project/vllm/pull/58847) — well-formed text parsing to zero calls
- [vLLM PR #54971](https://github.com/vllm-project/vllm/pull/54971) — arg markup leaking into the key
- [vLLM #42400](https://github.com/vllm-project/vllm/issues/42400) — Claude Code against GLM, same symptom, fixed server-side
- [vLLM tool-calling docs](https://github.com/vllm-project/vllm/blob/main/docs/features/tool_calling.md) — `glm45` / `glm47` parsers
- [vLLM #47504](https://github.com/vllm-project/vllm/issues/47504) — why `tool_choice: "required"` is the wrong fix
- [vLLM #36833](https://github.com/vllm-project/vllm/issues/36833) — text-only responses are a server misconfiguration
- [Qwen3-Coder template](https://huggingface.co/Qwen/Qwen3-Coder-30B-A3B-Instruct) — `<function=NAME><parameter=K>`, **not** `arg_key`
