# Byron Pfukwa Code Style (BPCS)

> **Job:** The official rulebook for Byron's code. Fast, flat, short. No noise.

## 1. Core Philosophy (The Systems Way)
* **Return Early (Guard Clauses):** Inspired by elite systems codebases like the original Bitcoin C++ source. Check for failure first. If it is bad, drop it immediately. Never use `else` if you can return early.
* **Silence is Golden:** Do not comment obvious code. Code must explain the *what*. 
* **Comment Only When Necessary:** Only write a statement when the logic is weird, handling a complex edge case, or when things are failing.
* **Flat over Nested:** Keep indentation minimal. Enter, do work, finish. 
* **Meaning, But Short:** Variable names must explain what they hold, but always chopped. No single-letter junk (except standard iterators). 

## 2. Commentary Rules (Direct African English)
No big English. Speak straight. Use these strictly for edge cases or failures:
* **Sorted:** All good / Success.
* **Make a plan:** Workaround / Fallback for failing logic.
* **Drop it:** Return early / Abort execution.
* **Is it there?:** Check existence before proceeding.

## 3. Standard Shortcuts

| Meaning | BPCS Shortcut | Example |
| :--- | :--- | :--- |
| User | `usr` | `usr_id` |
| Token | `tok` | `auth_tok` |
| Check / Validate | `chk` | `chk_sig` |
| Size / Length | `sz` | `blk_sz` |
| Error | `err` | `err_msg` |
| Buffer | `buf` | `raw_buf` |
| Request | `req` | `req_pl` |
| Transaction | `tx` | `tx_hash` |
| Block | `blk` | `blk_hdr` |
| Previous | `prev` | `prev_hash` |
| Message | `msg` | `err_msg` |

## 4. The Rules in Action

### Bad Example (Bloated, Nested, Over-commented)
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

### Byron Pfukwa Style (Go)
```go
func chk_tok(tok string) (*Usr, error) {
    if tok == "" {
        return nil, ErrEmpty // Drop it
    }

    if !is_valid(tok) {
        return nil, ErrBadSig // Drop it
    }

    usr, err := db.get_usr(tok)
    if err != nil {
        return nil, err // DB dead, make a plan later
    }

    return usr, nil // Sorted
}
```

### Byron Pfukwa Style (Rust / Systems)
```rust
pub fn proc_blk(buf: &[u8]) -> Result<(), Err> {
    let sz = buf.len();
    
    if sz < MIN_BLK_SZ {
        return Err(Err::TooSmall); // Drop it
    }
    
    let is_ok = chk_magic(buf);
    if !is_ok {
        return Err(Err::BadMagic);
    }

    // Buffer includes trailing junk from the network, slice before parse
    let clean_buf = &buf[..sz - JUNK_PAD]; 
    
    parse(clean_buf)?;

    Ok(()) // Sorted
}
```

## 5. The MD Signature Template
When documenting code in Markdown, always use this exact structure to frame the logic.

```markdown
# [Action] Logic
> **Job:** [What it does in one sentence. Keep it blunt.]

\`\`\`[language]
// BPCS code here
\`\`\`
```
