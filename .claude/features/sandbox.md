# Sandbox — Untrusted-Source Execution (plan, approved decisions locked)

Status: plan. Phases 1–4 not yet implemented.

## Locked decisions

- Unknown origin → treated as `untrusted` (user can mark trusted explicitly).
- Default untrusted profile → `offline` (no network unless user explicitly approves a netted run).

## Trust model

Levels: `trusted` | `untrusted` | `unknown` (handled as untrusted).

| Signal | Trusted | Untrusted |
|---|---|---|
| User declaration | "this is my repo", session toggle, pasted content | "check out this random repo", fetched URL |
| Origin provenance | user's own files, existing project dir | `web.read` output, fresh `git clone`, curl-piped content |
| Host | allowlist (`github.com/you/*`, self-hosted GitLab, in settings) | unknown hosts, raw IPs, shorteners — always untrusted |
| Content triggers | — | build/install scripts, `curl … \| sh` in fetched content |

Enforcement principle: the model never decides trust — it only carries provenance labels. The backend (`tools::exec` gate) re-derives trust from origin tags on every call. Ambiguous cases go to approval with a trust badge; the user is final arbiter.

## Mechanism: Podman (rootless containers)

Why Podman over bubblewrap: daemonless, rootless by default, Docker-compatible CLI, per-run resource limits, and a real filesystem boundary (image + volume) instead of namespace tricks over the host root. Fits the local-first model — no daemon, no root.

- Run shape: `podman run --rm --read-only --cap-drop=all --security-opt=no-new-privileges --pids-limit=256 --memory=2g --cpus=2 -v <repo>:/work:rw -w /work <image> <cmd>`. Existing timeout/output caps and `Channel` streaming unchanged — Podman wraps, doesn't replace.
- Profiles: `offline` (default, `--network=none`) for build/test/inspect; `netted` (bridge network, FS/process isolation only) strictly on explicit approval, since network re-opens exfiltration.
- Images: official per-language images pinned by digest (e.g. `rust:…@sha256:…`, `node:…@sha256:…`), image allowlist in settings. Never `:latest`, never user-suggested image refs — the model proposes the language, the backend maps it to the pinned image.
- Never silently downgrade: fallback chain is `podman` → `docker` CLI (same flags) → refuse with install hint. A weaker fallback would lie about the isolation guarantee, so absence of a container runtime is a hard stop, not a quiet bypass.

## Integration points

- New `src-tauri/src/tools/sandbox.rs`: `Trust` enum, `classify()`, `run_sandboxed()`.
- `src-tauri/src/tools/mod.rs`: provenance tags on executions + gate check.
- `src-tauri/src/sessions/chat.rs`: origin labels through the loop, trust in approval events.
- `src/stores/sessions.ts` + `ApprovalBlock.tsx`: trust badge + "Run sandboxed".
- Settings host + image allowlists via kv prefs; tests in `src-tauri/tests/sandbox_test.rs` (classification offline-testable, container runs presence-gated on `podman info`).

## Phases

1. Trust model + classification (pure logic, no deps).
2. Sandbox runner (detect runtime, offline profile, pinned images, hard refuse).
3. Loop + UI wiring (provenance, badge, run-sandboxed).
4. Hardening (netted policy, allowlist UI, docs).
