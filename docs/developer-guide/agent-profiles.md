# Agent profiles

> A profile is a persistent named identity that owns its chats, its sub-agents and
> its projects, and that can be granted limited visibility of and control over other
> profiles. One unnamed default exists; the user creates the rest.

**Status:** design agreed, not implemented.

**Open:** the user-facing name. This document uses *profile* because that is the
word used so far and the word on the button at `src/components/Toolbar.tsx:128`
(`aria-label="Profile"`, currently no `onClick`). `Desk` is the recommended
alternative — it carries all three required senses (a station, work that belongs to
it, and a floor of peers) and matches the company metaphor the feature is built on.
Whatever the UI word, the internals are `agent_profiles` / `profile_id`, which stay
distinct from the sandbox `profile` argument that `terminal` and `jobs` already use.

---

## 1. Scope

| Per profile | Global |
|---|---|
| Sessions (chats) | Providers, models — keyring `argus-gw` |
| Sub-agents spawned inside them | Theme |
| Projects (`folders`) | Skills |
| Reach (see §6) | Memory, and the session-memory graph |
| | Library — notes and documents agents create |
| | Connectors — keyring `argus-connector` / `-google` / `-github` |

A profile owns **no** provider, model, permission or theme setting. Those already
live where they live and are configured once. A profile's entire configuration is a
name and a set of instructions. Anything more would mean configuring the same thing
in two places.

Projects become per-profile. Everything else in the "per profile" column is either
already session-scoped or is added by this feature.

---

## 2. Schema

```sql
CREATE TABLE agent_profiles (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  instructions TEXT NOT NULL DEFAULT '',
  reach_all   INTEGER NOT NULL DEFAULT 0,
  created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

ALTER TABLE sessions ADD COLUMN profile_id     TEXT REFERENCES agent_profiles(id);
ALTER TABLE sessions ADD COLUMN profile_prompt TEXT;
ALTER TABLE folders  ADD COLUMN profile_id     TEXT REFERENCES agent_profiles(id);

CREATE TABLE profile_grants (
  profile_id TEXT NOT NULL REFERENCES agent_profiles(id) ON DELETE CASCADE,
  capability TEXT NOT NULL,
  target_id  TEXT NOT NULL,
  PRIMARY KEY (profile_id, capability, target_id)
);
```

Migration lives in `src-tauri/src/sessions/schema.rs` alongside the existing
`MIGRATE` string. There is no separate migration runner.

`reach_all = 1` means "every capability, every target". The `profile_grants` rows are
**kept, not deleted**, so flipping back to the custom matrix restores it. The user
who ticks two accountants, switches to All for a week, and switches back should not
have to re-tick.

The unnamed default profile is created at migration time with `name = ''` and is
never deletable. A session with `profile_id IS NULL` is treated as belonging to it,
so pre-existing chats keep working untouched.

---

## 3. Prompt layering

A profile's instructions are **added to** the base prompt, never substituted for it.
A profile that carried its own full system prompt would silently lose the terminal
rules, the sandbox fail-closed boundary, approval semantics, memory, recovery, and
the long-running-work section.

The existing user-authored text pattern in `stable_layer()`
(`src-tauri/src/prompt/mod.rs:84`) is the model to follow — the same wrapper is
reused for `preference_rules` at line 111:

```rust
s.push_str("\n\n<user-preferences>\n");
s.push_str("These are user preferences. Follow them when compatible with the core rules above.\n");
```

A profile gets the same treatment, one layer down. Resulting order:

```
BASE                                   safety, tools, memory, recovery — never profile-editable
+ <profile>                            name + instructions, subordinate to BASE
+ (user-preferences)                   unchanged, from kv `preference_rules`
+ tools::section(web)
+ <peers>                              only when the profile has any grant, see §6
+ <past-conversations>                 existing, already scoped by session
```

**A role label is not a mechanism.** A profile called "Senior Developer" that says
nothing behaves like base Argus with a friendlier tone. The instructions have to say
what to produce and what "done" means — *read the diff before commenting, quote
`file:line` for every claim, report rather than fix, state what you did not check.*
That is the difference between a code reviewer and a chatty developer.

### Base prompt defect found while designing this

`BASE` currently instructs, under `RESPONSE FORMAT`:

> Code fences and any other tags show up literally — avoid them.

The renderer supports `<codeblock language="...">` and emits a real `<pre>`
(`src/lib/agentXml.ts:17`, `src/components/AgentBubble.tsx:481`), and a markdown
` ``` ` fence is converted into one by `normalizeMd`. The prompt is simply wrong
about the app it runs in, and a developer profile is the exact case that proves it.
Fix this independently of profiles; it is a two-line change with a real blast radius
on every turn that shows code.

---

## 4. Snapshot semantics

At session creation, `profile_prompt` is frozen onto the session row. The prompt is
not read live from the profile.

Reasons:

- **Prompt cache.** The stable layer is the hashed cached prefix (`prefix_hash`,
  `src-tauri/src/prompt/mod.rs:288`). A live reference means editing one profile
  invalidates the cache for every session using it, mid-conversation.
- **Reproducibility.** A chat from March reads as it did. Editing a profile should
  not silently rewrite the behaviour of old work.
- **The lock is the point.** A session's instructions do not drift because somebody
  reworded a profile six weeks later.

Consequence: a profile cannot be changed for a conversation that already exists.
Editing the profile's text affects new sessions only, and the UI says so.

---

## 5. Sub-agents

A sub-agent inherits its parent's profile. The child's session row gets the same
`profile_id` and `profile_prompt` as its parent at `create_child` time.

This is one line and it is what the user expects — a Senior Developer chat that fans
out produces senior developers. It also compounds deliberately: a code reviewer that
spawns four children spawns four reviewers.

Sub-agents cannot reach peers of their own accord. A sub-agent's reach is the
parent profile's reach, and the sub-agent is not itself a grant target — grants name
profiles, and a sub-agent is a session, not a profile.

---

## 6. Reach

Reach is what one profile may do to another. It is a separate concept from
**approval** (`sessions.permission`, currently `ask` | `never`), which is whether a
human signs off on a tool action. The two axes are unrelated and must not share a
name.

### The ladder

Capabilities are ordered weakest to strongest. The ordering is the UI order and it
tells the user which one they are handing out.

| Capability | Grants | Why separate |
|---|---|---|
| `see_activity` | titles, state, liveness of another profile's sessions | "I can see you're busy" is a much smaller ask than "I can read your work" |
| `read_chats` | the actual transcripts | |
| `write_prompts` | put a prompt into another profile's chat | see §7 |
| `interrupt` | stop a running turn or a sub-agent | destructive |
| `edit` | change a profile's name and instructions | prompt injection into a peer, see §7 |
| `change_access` | change another profile's grants | the escalation path |

### The matrix

A master gate, off by default, reveals either an "All profiles" toggle or the
per-capability × per-target grid:

```
Give this profile access to others        [ off ]
   ○ All profiles    ● Specific profiles

                                 Manager  Accountant  Designer
  See their activity                 ☑           ☑           ☐
  Read their chats                   ☑           ☐           ☐
  Write prompts to them              ☐           ☐           ☐
  Interrupt their work               ☐           ☐           ☐
  Edit them                          ☐           ☐           ☐
  Change their access                ☐           ☐           ☐
```

Off by default matters more than it looks: a newly created profile reaches nothing,
so the safe state is the default state and there is no window in which a profile
exists with unexplained access.

Reach is **directional and sparse**. "The manager can message the accountant" grants
nothing in the other direction.

### Non-delegation

Two rules, enforced in the command layer and not merely in the UI:

1. A profile cannot grant a capability it does not itself hold.
2. A profile cannot modify its own grants.

Without these, `edit` is a back door: hold `edit` on the manager and rewrite the
manager's matrix to promote yourself. Only the human grants reach. Rule 1 is what
makes `change_access` safe to exist at all — it is the top of the ladder and it can
never be climbed by a peer.

---

## 7. Peer prompts are an injection path

Every other untrusted-input surface in the app is defended: web pages, screen text,
pasted documents are data, never instructions. A profile holding `write_prompts` on
another profile types **model-authored** text into that profile's chat, in the same
slot the human types in, where it will be obeyed.

This is a real path and the feature should not ship pretending otherwise. It does not
argue against the feature — a company where the manager cannot retask people is not
a company. It argues for three limits that bound the blast radius to what the
receiving profile could already do on its own authority:

- A peer prompt is **always labelled with the sending profile**. Never rendered as
  though the human sent it.
- A peer can **never** cause a permission change, regardless of what the matrix says.
  `change_access` is human-only.
- A peer prompt **cannot raise a profile's reach**, and the human's own instructions
  always outrank a peer's.

What this deliberately does **not** allow: a manager silently rewriting a
developer's instructions. `edit` is a real capability with a real risk, and the
mitigation is that the human granted it explicitly, to a named target, in a visible
matrix.

---

## 8. Liveness — unresolved

`see_activity` promises to say whether a profile is working *right now*. The
database can say what a session is called and when it was last touched. It cannot
say whether a turn is live: that lives in process memory (`gw.turns`, `gw.watching`)
and dies with the process.

`sessions.running_agents` covers only a session's own children and is no help here.
Three options, none chosen:

- **Heartbeat column.** A `last_heartbeat` written on turn start and end. Cross-desk
  reads become a single query, but a process that dies mid-turn leaves a row that
  looks live until it goes stale, and staleness needs its own threshold.
- **Bus broadcast.** Profiles publish state on the gateway bus. Accurate while the
  process lives, and simply absent after a crash — which is honest, but means a
  consumer cannot distinguish "not running" from "the app died".
- **Hybrid.** Heartbeat for the persisted truth, bus for immediacy.

This is the one item that should be settled before the read path is written, because
a manager acting on a stale liveness marker makes a wrong call on someone else's
work.

---

## 9. New tools

Following `ToolMeta` (`src-tauri/src/tools/mod.rs:19`). Every one of these refuses
outright when the grant is absent — it does not return an empty result, because a
silent empty list reads as "they are idle" rather than "you may not look".

| Tool | Mutating | Notes |
|---|---|---|
| `profile.list` | no | Profiles, and what each is working on |
| `profile.read` | no | A peer profile's transcript, requires `read_chats` |
| `profile.write` | **yes** | Prompt into a peer, requires `write_prompts` |
| `profile.stop` | **yes** | Interrupt a peer, requires `interrupt` |
| `profile.create` | **yes** | Requires `edit` **and** the ten-profile cap |
| `profile.grant` | **yes** | `change_access` — human-only in practice, see §6 |

`profile.write` is `mutating: true` and therefore subject to the normal approval gate
at `tools/mod.rs:693`. A manager prompting an accountant is settled in front of the
user like any other write, which is the desired behaviour under `ask`.

## 10. New commands

All in `src-tauri/src/agents/` or a new `src-tauri/src/profiles/`, registered in
`generate_handler!` (`src-tauri/src/lib.rs`, ~70 commands today), each with an
`ipc.ts` binding and a test in `src-tauri/tests/profiles_test.rs`.

```
profile_list  profile_create  profile_edit  profile_delete
profile_reach_get  profile_reach_set
profile_send  profile_stop
```

## 11. UI surfaces

- **Toolbar profile button** (`src/components/Toolbar.tsx:128`) — first destination.
  Opens the profile manager: list, create, edit instructions, reach matrix.
- **Chat header** — show the owning profile, with the profile name in the same
  position the session title occupies, so a chat is legible as *who is doing this*.
- **Sidebar** — group or filter sessions by profile. Folds into the existing
  Projects axis: a project is *what it is about*, a profile is *who is doing it*,
  and a session has exactly one of each.
- **New Agent page** — choose a profile when starting a chat.
- **Sub-agent page** — inherits, and says which profile it belongs to.

Projects and profiles both group sessions. The UI must not let them read as the same
axis. A project is a subject; a profile is an agent.

---

## 12. Limits

- **Ten profiles**, enforced at creation. Reuses the shape of `MAX_CHILDREN`
  (`src-tauri/src/agents/mod.rs:15`) and its error text.
- **Default profile** is created at migration, unnamed, and cannot be deleted.
- Sessions with `profile_id IS NULL` resolve to the default, so nothing that exists
  today needs migrating.

---

## 13. Tests

`src-tauri/tests/profiles_test.rs`, integration tests only — the repo keeps every
test out of `src/`. The ones that matter:

- a profile's instructions land in the system prompt and do **not** displace `BASE`
- a snapshot is taken at creation and survives a later edit to the profile
- a sub-agent inherits its parent's profile and prompt
- every capability is refused without the grant, and refused loudly
- a profile cannot grant a capability it does not hold
- a profile cannot modify its own grants
- a peer prompt is stored attributed to the sender and never as a human message
- a peer prompt cannot change any grant
- the eleventh profile is refused
- the default profile cannot be deleted
- a session with a null `profile_id` resolves to the default
