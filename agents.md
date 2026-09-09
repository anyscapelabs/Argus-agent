# Byron Pfukwa Code Style (BPCS) — v2

> **Job:** The official rulebook for Byron's code, now covering Go, Rust, and TypeScript. Fast, flat, short. No noise.

**What's new in v2:** locked to three languages (Go, Rust, TypeScript), checked against real naming and layout conventions from the Rust API Guidelines, the Uber Go Style Guide, the Google TypeScript Style Guide, and the Deno Style Guide — so BPCS code reads like it belongs in those codebases, not like it's fighting their linters. Full sources at the bottom.

## Table of Contents

1. [Core Philosophy (The Systems Way)](#1-core-philosophy-the-systems-way)
2. [The Casing Law](#2-the-casing-law)
3. [Commentary Rules (Direct African English)](#3-commentary-rules-direct-african-english)
4. [Standard Shortcuts](#4-standard-shortcuts)
5. [Whitespace & Layout Law](#5-whitespace--layout-law)
6. [The Rules in Action](#6-the-rules-in-action)
7. [The MD Signature Template](#7-the-md-signature-template)
8. [Quick Reference Cheat Sheet](#8-quick-reference-cheat-sheet)
9. [Sources](#9-sources)

---

## 1. Core Philosophy (The Systems Way)

* **Return Early (Guard Clauses):** Inspired by elite systems codebases like the original Bitcoin C++ source. Check for failure first. If it is bad, drop it immediately. Never use `else` if you can return early.
* **Silence is Golden:** Do not comment obvious code. Code must explain the *what*.
* **Comment Only When Necessary:** Only write a statement when the logic is weird, handling a complex edge case, or when things are failing.
* **Flat over Nested:** Keep indentation minimal. Enter, do work, finish.
* **Meaning, But Short:** Variable names must explain what they hold, but always chopped. No single-letter junk (except standard iterators).

**Field note:** rule 1 isn't just a Byron preference — it's enforced tooling in a lot of serious codebases. LLVM's coding standard bans `else` after a `return`/`break`/`continue`/`throw` outright, and the linter Biome ships a rule (`noUselessElse`) that exists purely to auto-fix that exact pattern. BPCS didn't invent guard clauses; it just refuses to compromise on them.

---

## 2. The Casing Law

BPCS keeps one shortcut vocabulary across all three languages (Section 4), but casing is **not** up for negotiation — each language has its own compiler- or tooling-enforced law, and code that ignores it looks foreign to any reviewer in that ecosystem, chopped names or not.

| Construct | Go | Rust | TypeScript |
|---|---|---|---|
| Local var / param | `snake_case` | `snake_case` | `camelCase` |
| Private fn / method | `snake_case` | `snake_case` | `camelCase` |
| Public fn / method | `PascalCase` | `snake_case` | `camelCase` |
| Type / Struct / Class | `PascalCase` | `PascalCase` | `PascalCase` |
| Interface / Trait | `PascalCase`, no `I` prefix | `PascalCase`, no `I` prefix | `PascalCase`, no `I` prefix |
| Constant / Static | `SCREAMING_SNAKE_CASE` | `SCREAMING_SNAKE_CASE` | `SCREAMING_SNAKE_CASE` |
| Error type | `ErrXxx` var, never `XxxErr` | `XxxError` variant, never bare `Err` | `XxxError extends Error` |
| File name | `snake_case.go` | `snake_case.rs` | `kebab-case.ts` (pick one, stay consistent) |

**Field notes:**
- **Go — capital = exported is compiler law, not style.** An identifier starting with an uppercase letter is visible outside its package; a lowercase one isn't. This is why Byron's original example calling `db.get_usr(tok)` across a package boundary was a bug waiting to happen — it has to be `db.GetUsr(tok)`. Fixed in Section 6.
- **Rust — RFC 430 draws the line cleanly:** `snake_case` for values (functions, modules, variables), `UpperCamelCase` for types and traits, `SCREAMING_SNAKE_CASE` for consts and statics. Don't name an error enum `Err` either — it collides visually with `Result::Err`, and `Err::Err(Err::TooSmall)` is not a sentence anyone wants to review. Call it `Error` and alias `Result<T>` at the crate level instead (Section 6).
- **TypeScript — camelCase isn't optional either.** Google's and Deno's style guides both mandate it for values, and `@typescript-eslint/naming-convention` will flag anything else in most real projects. BPCS shortcuts still apply — `usr_id` in Go/Rust becomes `usrId` in TypeScript — the vocabulary travels, the casing doesn't.

---

## 3. Commentary Rules (Direct African English)

No big English. Speak straight. Use these strictly for edge cases or failures.

### The Core Four
* **Sorted:** All good / Success.
* **Make a plan:** Workaround / Fallback for failing logic.
* **Drop it:** Return early / Abort execution.
* **Is it there?:** Check existence before proceeding.

### v2 Additions
* **Loud fail:** Deliberate panic/crash — used only when continuing would be worse than dying (bad config at boot, a broken invariant). Mirrors Go's convention of reserving `panic` for truly unrecoverable startup errors and Rust's `.expect()` for invariants you'd stake the program on.
* **Not my problem:** Bubble the error up untouched — no logging, no wrapping, just hand it to a caller who actually knows what to do with it. Matches the "handle an error once" rule that shows up in the Uber Go guide: don't log an error *and* return it, callers up the stack will just do the same thing again.
* **Still cooking:** Work in progress, don't ship yet. For `TODO`-style markers on logic that compiles but isn't trusted.

---

## 4. Standard Shortcuts

Same vocabulary, every language. Casing follows Section 2.

### Identity & Access
| Meaning | Shortcut | Example |
| :--- | :--- | :--- |
| User | `usr` | `usr_id` |
| Token | `tok` | `auth_tok` |
| Identifier | `id` | `tx_id` |
| Authentication | `auth` | `auth_hdr` |

### Data & Buffers
| Meaning | Shortcut | Example |
| :--- | :--- | :--- |
| Buffer | `buf` | `raw_buf` |
| Size / Length | `sz` | `blk_sz` |
| Payload | `pl` | `req_pl` |
| Temporary | `tmp` | `tmp_buf` |

### Networking & I/O
| Meaning | Shortcut | Example |
| :--- | :--- | :--- |
| Request | `req` | `req_pl` |
| Response | `res` | `api_res` |
| Connection | `conn` | `db_conn` |
| Context | `ctx` | `req_ctx` |
| Address | `addr` | `peer_addr` |

### State & Control
| Meaning | Shortcut | Example |
| :--- | :--- | :--- |
| Configuration | `cfg` | `app_cfg` |
| Options | `opts` | `db_opts` |
| Default | `def` | `def_cfg` |
| Index | `idx` | `row_idx` |
| Count | `cnt` | `retry_cnt` |
| Check / Validate | `chk` | `chk_sig` |
| Initialize | `init` | `init_cfg` |

### Errors & Diagnostics
| Meaning | Shortcut | Example |
| :--- | :--- | :--- |
| Error | `err` | `err_msg` |
| Message | `msg` | `log_msg` |

### Ledger Set (Bitcoin lineage)
| Meaning | Shortcut | Example |
| :--- | :--- | :--- |
| Transaction | `tx` | `tx_hash` |
| Block | `blk` | `blk_hdr` |
| Previous | `prev` | `prev_hash` |
| Header | `hdr` | `blk_hdr` |

**Field note:** `opts` isn't just a Byron abbreviation — Deno's own style guide requires exported functions to cap out at two positional params and stuff the rest into an options object. The shortcut and the pattern happen to line up.

---

## 5. Whitespace & Layout Law

Flat over nested applies to the page, not just the logic.

| | Go | Rust | TypeScript |
|---|---|---|---|
| Indent | Tabs — `gofmt` enforces this, it's not a choice | 4 spaces (`rustfmt` default) | 2 spaces (`prettier` / ecosystem default) |
| Brace style | Same line, always — Go's grammar requires it | Same line (`rustfmt` default) | Same line |
| Line width | `gofmt` doesn't wrap; keep it sane by eye | 100 cols (`rustfmt` default) | 80 cols (`prettier` default) |
| Import order | stdlib, blank line, then third-party/local (`goimports`) | `std`, blank line, external crates, blank line, crate-local | external packages, blank line, internal/relative |
| Line endings | Semicolons handled by the compiler | Semicolons required | Semicolons required — don't lean on ASI |

**BPCS blank-line rules (all three languages):**
- One blank line, never two in a row.
- No blank line as the first or last line inside a `{ }` block.
- One blank line between the "beats" of a function — guard clauses, then setup, then core logic, then return. Not more, not less.

If the language has an official formatter (`gofmt`, `rustfmt`, `prettier`), run it. BPCS is about naming and shape, not about winning fights the formatter already won.

---

## 6. The Rules in Action

### Go

Inspired by: the Uber Go Style Guide, `gofmt`, and the Go spec's export rule.

**Bad Example (Bloated, Nested, Over-commented)**
```go
// This function verifies the user token and then fetches the user data
func authenticateUserToken(token string) (*User, error) {
    if token != "" {
        // Token is not empty, proceed to verify
        isValid := verifyToken(token)
        if isValid {
            user, err := database.FindUserByToken(token)
            if err == nil {
                return user, nil
            } else {
                return nil, err
            }
        } else {
            return nil, errors.New("invalid")
        }
    }
    return nil, errors.New("empty token")
}
```

**Byron Pfukwa Style (Go)**
```go
var ErrEmptyTok = errors.New("empty tok")
var ErrBadSig = errors.New("bad sig")

func chk_tok(tok string) (*Usr, error) {
    if tok == "" {
        return nil, ErrEmptyTok // Drop it
    }

    if !is_valid(tok) {
        return nil, ErrBadSig // Drop it
    }

    usr, err := db.GetUsr(tok)
    if err != nil {
        return nil, fmt.Errorf("get usr: %w", err) // make a plan later
    }

    return usr, nil // Sorted
}
```
Note `db.GetUsr`, not `db.get_usr` — `db` is a different package, so the call has to cross the export boundary in capitals. Note also `ErrEmptyTok`/`ErrBadSig`, not `EmptyTokErr` — sentinel errors read `ErrXxx`, always.

---

### Rust

Inspired by: the Rust API Guidelines / RFC 430, and the `thiserror` + `anyhow` split that most production crates settle on (`thiserror` for libraries, `anyhow` at the application edge).

**Bad Example (Bloated, Nested, Swallows the Real Error)**
```rust
// This function processes a block of data received from the network
fn process_block(buffer: Vec<u8>) -> bool {
    let length = buffer.len();
    if length >= MIN_BLOCK_SIZE {
        let magic_check = check_magic_bytes(&buffer);
        if magic_check == true {
            let cleaned = buffer[0..length - JUNK_PADDING].to_vec();
            match parse_block(&cleaned) {
                Ok(_) => {
                    return true;
                }
                Err(_) => {
                    return false;
                }
            }
        } else {
            return false;
        }
    } else {
        return false;
    }
}
```

**Byron Pfukwa Style (Rust)**
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlkError {
    #[error("blk too small")]
    TooSmall,
    #[error("bad magic")]
    BadMagic,
}

pub type Result<T> = std::result::Result<T, BlkError>;

pub fn proc_blk(buf: &[u8]) -> Result<()> {
    let sz = buf.len();

    if sz < MIN_BLK_SZ {
        return Err(BlkError::TooSmall); // Drop it
    }

    if !chk_magic(buf) {
        return Err(BlkError::BadMagic); // Drop it
    }

    // buf carries trailing junk from the wire, slice it off before parse
    let clean_buf = &buf[..sz - JUNK_PAD];

    parse(clean_buf)?;

    Ok(()) // Sorted
}
```
The bad version collapses a real, typed error into `bool` — the caller has no idea *why* it failed. The BPCS version keeps the failure reason alive all the way to whoever calls `proc_blk`, which is the whole point of `Result`.

---

### TypeScript

Inspired by: the Google TypeScript Style Guide and the Deno Style Guide.

**Bad Example (Bloated, Nested, Over-commented)**
```typescript
// This function checks the user's token and then gets their data from the database
async function authenticateUserToken(token) {
    if (token !== "") {
        // Token is not empty, so we proceed to verify it
        const isValid = await verifyToken(token);
        if (isValid) {
            const user = await database.findUserByToken(token);
            if (user) {
                return user;
            } else {
                throw new Error("user not found");
            }
        } else {
            throw new Error("invalid signature");
        }
    } else {
        throw new Error("empty token");
    }
}
```

**Byron Pfukwa Style (TypeScript)**
```typescript
class EmptyTokError extends Error {}
class BadSigError extends Error {}

async function chkTok(tok: string): Promise<Usr> {
    if (tok === "") {
        throw new EmptyTokError("empty tok"); // Drop it
    }

    if (!isValid(tok)) {
        throw new BadSigError("bad sig"); // Drop it
    }

    const usr = await db.getUsr(tok);
    if (!usr) {
        throw new Error("usr not found"); // Drop it
    }

    return usr; // Sorted
}
```
Note the `function` keyword at the top level, not an arrow — Deno's style guide reserves arrow syntax for closures and keeps top-level functions on `function` for hoisting and legibility. Note also there's no `else` anywhere: each guard throws and moves on, same shape as the early returns in the Go and Rust versions above.

**Field note:** throwing inside `chkTok` is still flat — it's the same shape as an early return, no nesting added. The nesting cost shows up one level up, at whatever calls `chkTok`, which now needs a `try/catch`. If a chain of calls matters as much as any single function, some teams return a small `{ ok: true, value } | { ok: false, error }` union instead of throwing, keeping the whole chain flat — the TypeScript equivalent of Go's `if err != nil` or Rust's `?`. BPCS doesn't mandate this, but it's worth knowing the trade-off exists.

---

## 7. The MD Signature Template

When documenting code in Markdown, always use this exact structure to frame the logic.

```markdown
# [Action] Logic
> **Job:** [What it does in one sentence. Keep it blunt.]

\`\`\`[language]
// BPCS code here
\`\`\`
```

**Spacing rules for BPCS docs:**
- No blank line between the `#` title and the `> **Job:**` line — they're one unit.
- Exactly one blank line before the code fence.
- Always tag the fence with a language (`go`, `rust`, `typescript`) — untagged fences don't get syntax highlighting and don't belong in a systems-style doc.
- Nothing after the closing fence. The code is the last word — no summary paragraph re-explaining what was just shown.

---

## 8. Quick Reference Cheat Sheet

**Rule 1, always:** guard clause, drop it, never `else`.

| | Go | Rust | TypeScript |
|---|---|---|---|
| Values | `snake_case` | `snake_case` | `camelCase` |
| Types | `PascalCase` | `PascalCase` | `PascalCase` |
| Constants | `SCREAMING_SNAKE_CASE` | `SCREAMING_SNAKE_CASE` | `SCREAMING_SNAKE_CASE` |
| Indent | tabs | 4 spaces | 2 spaces |
| Errors | `ErrXxx` sentinel | `XxxError` enum + `thiserror` | `XxxError extends Error` |

**Comment words:** Sorted · Make a plan · Drop it · Is it there? · Loud fail · Not my problem · Still cooking

**Top shortcuts:** `usr` `tok` `chk` `sz` `err` `buf` `req` `res` `ctx` `cfg` `opts` `tx` `blk` `prev` `msg`

---

## 9. Sources

Real conventions this edition was checked against, not just vibes:

- [Rust API Guidelines — Naming](https://rust-lang.github.io/api-guidelines/naming.html)
- [Rust API Guidelines — Checklist](https://rust-lang.github.io/api-guidelines/checklist.html)
- [Uber Go Style Guide](https://github.com/uber-go/guide/blob/master/style.md)
- [Google TypeScript Style Guide](https://google.github.io/styleguide/tsguide.html)
- [Deno Style Guide](https://docs.deno.com/runtime/contributing/style_guide/)
- [`anyhow` crate docs](https://crates.io/crates/anyhow)
- [Biome — `noUselessElse` lint rule](https://v1.biomejs.dev/linter/rules/no-useless-else)
- [LLVM / clang-tidy — `readability-else-after-return`](https://clang.llvm.org/extra/clang-tidy/checks/readability/else-after-return.html)
