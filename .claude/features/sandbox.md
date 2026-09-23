# Sandbox — native OS isolation (no container)

Status: implemented. Linux verified end to end (`tests/sandbox_linux_test.rs`,
13 tests, all passing). macOS SBPL generation and Windows JobPlan are
unit-tested from Linux; their syscall invocation is not.

## The decision

Argus does not run a container. It creates a controlled execution boundary
around individual processes, using whatever the OS already offers. The old
Podman path (`detect_runtime`, image allowlist, digest pins, `podman → docker →
refuse` fallback) is deleted.

## The split

Sandboxing must be installed between fork and exec, where only async-signal-safe
calls are legal. That forces the whole policy to be computed before the fork —
which is also what makes the other two platforms testable from Linux.

```
resolve(profile, ctx) -> Policy   policy.rs   pure, no I/O
plan(policy) -> Plan             plan.rs     pure, OS-free plan structs
apply(plan, cmd) -> Guard        backends/   the only platform-gated code
```

macOS SBPL strings, Linux Landlock rule lists, and Windows job-limit structs
are all plain data, so `tests/sandbox_test.rs` asserts them on any host.

## Trust model (unchanged, `trust.rs`)

Levels: `trusted` | `untrusted` | `unknown` (handled as untrusted).

| Signal | Trusted | Untrusted |
|---|---|---|
| User declaration | "this is my repo", session toggle, pasted content | "check out this random repo", fetched URL |
| Origin provenance | user's own files, existing project dir | `web.read` output, fresh `git clone`, curl-piped content |
| Host | allowlist (`github.com/you/*`, self-hosted GitLab, in settings) | unknown hosts, raw IPs, shorteners — always untrusted |
| Content triggers | — | build/install scripts, `curl … \| sh` in fetched content |

The model never decides trust; it carries provenance labels. The gate
re-derives trust from origin tags on every call. Ambiguous cases go to approval
with a trust badge; the user is final arbiter.

## Profiles (`policy.rs`)

| | Restricted | Project | Host |
|---|---|---|---|
| Filesystem | sysroot + `/dev /proc /sys /run` ro, tmp rw | sysroot ro, project + `~/.cache` + tmp rw, dep caches ro | unrestricted |
| Network | none | 80, 443 | full |
| Env | 7 keys only | inherit | inherit |
| Wall / mem | 120s / 2 GiB | 900s / 4 GiB | — |
| Output cap | 64 KiB | 256 KiB | 8 MiB |

`terminal` defaults to Host (behavior unchanged); the agent may pass
`profile: "project" | "restricted"` per command. `code.run` is always
Restricted.

## Per-platform enforcement (`backends/`)

**Linux** — Landlock LSM for filesystem and (ABI v4 / kernel 6.7+) TCP
restrictions, `seccompiler` BPF for a Restricted deny list, `setrlimit` for
AS/CPU/NOFILE/FSIZE, `setpgid(0,0)` so the teardown killpg takes the whole
tree. `PR_SET_NO_NEW_PRIVS` before any of it.

Two things the platform cannot express, and the code says so instead of
pretending:

- `RLIMIT_NPROC` counts every process of the real uid, so `linux_plan()` zeroes
  `procs` — a per-sandbox budget needs cgroups. The plan struct records `None`
  rather than a limit that does not exist.
- `RLIMIT_FSIZE` is a real file cap, not temp-storage accounting.

**macOS** — `sandbox-exec -p '<SBPL>'`, SBPL generated in `plan()` as a pure
string. `quote()` escapes `"` and `\` and strips newlines so a project path
cannot break out of the profile. Apple deprecated `sandbox-exec`; if it goes
away, isolated profiles fail rather than fall through to the host.

**Windows** — Job Objects for `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`,
`PROCESS_MEMORY`, `ACTIVE_PROCESS`. AppContainer is *not* wired: it needs
`STARTUPINFOEXW` on a raw `CreateProcessW`, which `tokio::process::Command` does
not expose. So `require_container()` refuses any plan with a non-empty
`fs_grants`. On Windows today, an fs-scoped profile is a hard stop — the
restricted filesystem boundary is not yet real, and the code fails closed
instead of downgrading.

## Fail closed

No path exists from sandboxed to host. `plan()` or `apply()` erroring returns
before the command runs; `a_refused_plan_never_runs_the_command` asserts the
command provably did not execute. `recover.rs` classifies every sandbox refusal
as `PermissionDenied` so the agent cannot retry into a weaker path.

## Observability (`record.rs`, `schema.rs`)

`sandbox_runs` holds id, tool, command, profile, backend, origin, permission,
start, duration, exit, termination, byte counts, truncated flag. **Output is
never persisted** — the audit trail is not a second place secrets live. Table
capped at `MAX_ROWS = 5000`.

## Tests

- `tests/sandbox_test.rs` — trust, policy resolution, macOS SBPL, Windows
  JobPlan, Linux plan, fail-closed errors, config round-trip. Runs everywhere.
- `tests/sandbox_linux_test.rs` — 13 tests, Linux only. Landlock read/write
  scope, network denial, seccomp denial, rlimit AS, no_new_privs, foreign-plan
  refusal, refused-plan-never-runs, Unsupported backend, killpg teardown.
