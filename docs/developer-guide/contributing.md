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

## Before you push

CI runs on every push to `master` and every pull request, and it runs the same
commands listed above — plus `clippy --all-targets -- -D warnings` and
`bun test`. Zero clippy warnings is the rule, not a suggestion, so the fastest
way to a green build is to run the gate locally first.

Use `cargo test --no-fail-fast`. One failing test binary otherwise stops the
run and hides the other 71. See [Releasing](releasing.md) for the one test that
is load-sensitive and will fail spuriously on a busy machine.

## Rules

- Production code carries no debug output (`console.*`, `println!`, `eprintln!`, `dbg!`) — those belong in tests.
- Production code panics never: return `Result`, use `let-else` guards, keep functions flat.
- Tests live in `src-tauri/tests/`, never inside `src/` files.
- TypeScript: `camelCase` values, `PascalCase` types, `SCREAMING_SNAKE_CASE` constants, top-level `function` declarations.
- Rust: `snake_case` values, `PascalCase` types, `SCREAMING_SNAKE_CASE` consts, `rustfmt` clean.
- Never commit secrets, tokens, or `config.json` files (gitignored per service).
