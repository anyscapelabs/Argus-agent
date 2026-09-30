# Releasing

> **Job:** how a build gets from a commit to someone's machine, and what to
> do when the certificate situation changes.

## Version scheme

The version lives in four files and CI fails if they disagree:

| File | Field |
|---|---|
| `package.json` | `version` |
| `src-tauri/Cargo.toml` | `package.version` |
| `src-tauri/tauri.conf.json` | `version` |
| `README.md` | the version badge |

The scheme is semver with a pre-release suffix:

- **`0.2.0-alpha.N`** — the private loop. Built from `master`, released to
  people we can talk to directly. Marked as a GitHub **prerelease**, so it is
  visually distinct from anything we stand behind.
- **`0.3.0`** — the public cut. The first release we ask strangers to install.

A tag with a `-alpha` suffix becomes a prerelease automatically; there is no
separate flag to remember.

## Cutting a release

```bash
# 1. Bump all four files to the new version, and say so in CHANGELOG.md.
# 2. Make sure CI is green on master.
git checkout master && git pull
git tag v0.2.0-alpha.1
git push origin v0.2.0-alpha.1
```

Pushing the tag is the entire trigger. `.github/workflows/release.yml` builds
macOS (Apple Silicon and Intel), Linux, and Windows, attaches the artifacts to
a GitHub Release, and marks it a prerelease.

If a build fails on one platform, re-run just that job from the Actions tab
rather than re-tagging. To rebuild a different tag, use the workflow's
`workflow_dispatch` input.

## Signing, and why testers still see a warning

The current builds are signed, but the signature carries no trust. This is
deliberate for now and worth being precise about, because "signed" and "trusted"
are different things and only one of them is true:

| | Now | With a real certificate |
|---|---|---|
| macOS | Ad-hoc identity (`-`). Apple Silicon will not run an unsigned binary at all, so *something* has to sign it. | Developer ID + notarization: no prompts, ever. |
| Windows | Self-signed, generated per build. | A CA or Azure Trusted Signing cert. SmartScreen still warns until the binary accumulates reputation, but stops saying "unknown publisher". |

Neither platform reaches a user who has never seen Argus without a click. The
release body spells out the bypass, because a download that warns and a
download that is malicious look identical until the user reads.

**What self-signing does buy** is the whole pipeline: the signtool invocation,
the thumbprint plumbing, the timestamping, the ad-hoc identity. When a real
certificate arrives it is a change to two steps in `release.yml` and three
fields in `tauri.conf.json`. Nothing else moves.

### Moving to real signing

**macOS** — enrol in the Apple Developer Program (~$99/yr), then export a
Developer ID Application certificate as a `.p12` and add three repository
secrets: `APPLE_CERTIFICATE` (base64), `APPLE_CERTIFICATE_PASSWORD`, and
`APPLE_SIGNING_IDENTITY`. Set `APPLE_ID`, `APPLE_APP_SPECIFIC_PASSWORD`, and
`APPLE_TEAM_ID` to enable notarization. A free Apple account cannot notarize.

**Windows** — either a CA certificate (~$300–700/yr, often a hardware token) or
Azure Trusted Signing (~$400/yr, no token, needs identity validation). Replace
the `Create a self-signed code-signing certificate` step with a PFX import from
a secret; the config-file step is unchanged.

## What CI enforces

`.github/workflows/ci.yml` runs on every push to `master` and every pull
request:

- the four version fields agree
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings` — zero warnings is
  the invariant from `.claude/rules/rust.md`, so it is enforced as an error
- `cargo test --no-fail-fast`
- `tsc --noEmit`, `bun test`, `bun run build`

`--no-fail-fast` is there because one failing test binary otherwise hides the
other 71.

### The one test that lies

`browser_shown_test::profile_isolation_for_shown_watermark` polls a
process-global for 5 seconds and is load-sensitive. On a busy runner it fails;
in isolation it passes. If it goes red, re-run before you believe it — and if it
keeps failing on CI specifically, that is a real problem worth fixing rather
than retrying, because CI is always loaded.

## The Chrome extension

Not packaged with the app. Testers load it manually from `extension/` via
`chrome://extensions` → Developer mode → Load unpacked. It is three files with
no build step, and browser tools need it present to do anything.
