# Roadmap

Planned work, roughly in priority order. Shipped items live in the
[CHANGELOG](CHANGELOG.md). This list is intentionally scoped — see
**Deferred** at the bottom for things considered but parked.

Detailed, self-contained implementation briefs for each item (plus newly
scoped features) live in [IMPLEMENTATION-BRIEFS.md](IMPLEMENTATION-BRIEFS.md).

## CLI polish (cheap, high-value)

- [ ] **Shell completions** — `squish completions <bash|zsh|fish>` via
  `clap_complete`. zsh is the priority target. Generated from the existing
  clap `Args`, so it stays in sync automatically. Ship install hints in the
  README and, where practical, have the Homebrew formula drop the zsh
  completion into `#{zsh_completion}`.
- [ ] **Man page** — generate `squish.1` with `clap_mangen` (a build script or
  a hidden `squish --man` subcommand), install it via the formula's
  `man1.install`.
- [ ] **`cargo binstall` support** — add `[package.metadata.binstall]` to
  `squish-cli` mapping the release artifact naming
  (`squish-vX.Y.Z-<target>.tar.gz`) so `cargo binstall squish-media-cli`
  fetches a prebuilt binary instead of compiling.
- [ ] **`--json` output mode** — machine-readable run summary (per-file and
  totals: bytes in/out, format, saving, errors). Pairs with the GitHub Action
  so workflows can post a "saved X MB" PR comment, and makes squish
  scriptable.

## Maintenance & hygiene

- [ ] **Supply-chain CI** — add `cargo-audit` (RUSTSEC advisories) and
  `cargo-deny` (licences + bans + duplicate versions) as a CI job.
- [ ] **Dependabot** — `.github/dependabot.yml` for both `cargo` and
  `github-actions` ecosystems, so dependency and Action-version bumps arrive
  as PRs (which CI then gates).
- [ ] **MSRV CI job** — a dedicated job pinned to Rust 1.95 running
  `cargo check --workspace`, so an accidental use of a newer-stable API is
  caught instead of silently raising the real MSRV.
- [ ] **Compression-ratio regression guard** — golden-number tests asserting
  each fixture shrinks by at least a known threshold (e.g. `sample.png ≥ 70%`).
  Catches a dependency bump that quietly *worsens* output — the failure mode
  behind the v0.3.3 usvg→oxvg SVG regression.
- [ ] **Unwind the `=` dependency pins** — `lightningcss` and `minify-html` are
  hard-pinned in `squish-code` because `oxvg_optimiser 0.0.5` unconditionally
  enables lightningcss's removed `grid` feature. Revisit when `oxvg` publishes
  a release past 0.0.5 (still the latest as of 2026-06). The Dependabot/audit
  work above should surface this automatically.

## Encoding quality

- [ ] **Two-pass video encoding for `--target-size`** — replace (or back) the
  current single-pass-ABR-with-retry loop with ffmpeg's native two-pass mode
  for more accurate size targeting on a known budget. Keep the retry loop as a
  fallback for codecs/containers where two-pass is awkward.

## Deferred

Considered and intentionally parked for now — revisit if demand appears:

- **Windows support** (binaries, `windows-latest` CI, Scoop manifest) — low
  priority while the audience is macOS/Linux-first.
- **Docker image** — not needed yet given prebuilt binaries + the GitHub
  Action cover the CI use case.
