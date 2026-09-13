# Contributing
> **Job:** Build, test, and style rules for contributors.

## Commands

```bash
bun install        # frontend deps
bunx tauri dev     # desktop dev loop
npx tsc --noEmit   # typecheck frontend
cargo fmt          # format backend (required before push)
cargo check        # fast backend check
cargo test         # full backend suite (src-tauri/)
```

## Rules

- Production code carries no debug output (`console.*`, `println!`, `eprintln!`, `dbg!`) — those belong in tests.
- Production code panics never: return `Result`, use `let-else` guards, keep functions flat.
- Tests live in `src-tauri/tests/`, never inside `src/` files.
- TypeScript: `camelCase` values, `PascalCase` types, `SCREAMING_SNAKE_CASE` constants, top-level `function` declarations.
- Rust: `snake_case` values, `PascalCase` types, `SCREAMING_SNAKE_CASE` consts, `rustfmt` clean.
- Never commit secrets, tokens, or `config.json` files (gitignored per service).
