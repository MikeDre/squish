# Implementation Briefs

Detailed, self-contained work briefs expanding on [ROADMAP.md](ROADMAP.md), written so
each item can be picked up and implemented independently without extra context. Each
brief states the goal, the files involved, the spec, acceptance criteria, and pitfalls.

**Read the [Project conventions](#project-conventions) section before starting any brief.**

---

## Project conventions

These apply to every brief below.

- **Workspace layout**: six crates under `crates/` — `squish-core` (images),
  `squish-video`, `squish-audio`, `squish-media` (shared ffmpeg plumbing),
  `squish-code` (minifiers), `squish-cli` (binary, package name
  `squish-media-cli`, binary name `squish`).
- **Branching**: one branch per feature, named `feat/<slug>`, `chore/<slug>`, or
  `docs/<slug>`. Merge to `main` with a merge commit titled
  `Merge feat/<slug>: <short description>` (see `git log` for examples).
- **Commit style**: emoji-prefixed, imperative-ish one-liners:
  - `✨ Added: <feature>` — new functionality
  - `✅ Added: <test description>` — tests
  - `📝 Updated: <docs change>` — docs
  - `🔧 Updated: <chore/config>` — chores
  - `🐛 Fixed: <bug>` — fixes
  - Never add a `Co-Authored-By` trailer.
- **Tests first**: write the failing test, watch it fail, then implement. CLI
  integration tests live in `crates/squish-cli/tests/cli_tests.rs` and MUST invoke
  the binary through the existing `bin()` helper (it sets `SQUISH_NO_STATS=1`; a
  meta-test enforces this). Per-crate round-trip tests live in each crate's
  `tests/round_trip.rs` with fixtures in `tests/fixtures/`.
- **MSRV is Rust 1.95** (`rust-version` in the workspace `Cargo.toml`). Don't use
  newer-stable APIs.
- **Dependency pins**: `lightningcss` and `minify-html` are `=`-pinned in
  `squish-code` (forced by `oxvg_optimiser 0.0.5`). Do not unpin them as a side
  effect of other work — see Brief 16.
- **Docs**: every user-visible change updates `README.md` (including the Flags
  block, which mirrors `--help`) and gets a `CHANGELOG.md` entry under an
  `Unreleased`/next-version heading. Keep-a-Changelog format, SemVer.
- **CI must stay green**: `cargo test --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`.
- **Definition of done** for every brief: tests pass, clippy/fmt clean, README +
  CHANGELOG updated, ROADMAP.md checkbox ticked (or item removed).

---

## Phase 1 — Hygiene & quick wins

Low-risk, high-value. No ordering constraints within the phase; all can be done in
parallel on separate branches.

### Brief 1: Dependabot configuration — size XS

**Goal**: automated dependency-bump PRs for cargo and GitHub Actions.

**Do**: create `.github/dependabot.yml` with two update blocks:
- `package-ecosystem: cargo`, `directory: /`, weekly schedule, PR limit ~5,
  grouped minor+patch updates (use a `groups:` block so routine bumps arrive as
  one PR).
- `package-ecosystem: github-actions`, `directory: /`, weekly.

**Pitfalls**: exclude nothing, but note the `=`-pinned `lightningcss` /
`minify-html` in `crates/squish-code/Cargo.toml` — Dependabot will propose bumps
that CI will rightly fail; that is desired signal, not a problem to suppress.

**Acceptance**: file is valid YAML (`dependabot.yml` schema), CI unaffected.

### Brief 2: Supply-chain CI (cargo-audit + cargo-deny) — size S

**Goal**: fail CI on RUSTSEC advisories, licence violations, and banned/duplicate
dependencies.

**Do**:
1. Add a `deny.toml` at the repo root: `[licenses]` allowlist (MIT, Apache-2.0,
   BSD-2/3-Clause, ISC, Zlib, Unicode-3.0, MPL-2.0 as needed — run
   `cargo deny check licenses` locally and add what the current tree actually
   uses), `[advisories]` default settings, `[bans]` with `multiple-versions =
   "warn"` initially.
2. Add a new job `supply-chain` to `.github/workflows/ci.yml` (runs on
   `ubuntu-24.04`, no system deps needed since it doesn't compile):
   `EmbarkStudios/cargo-deny-action@v2` covers advisories + licences + bans in
   one step. No separate cargo-audit step needed if deny's advisories check runs.

**Acceptance**: the job passes on current `main`; deliberately adding a
GPL-licensed dummy dep locally makes `cargo deny check` fail.

**Pitfalls**: don't gate the job on the libheif build steps — it must stay fast
(< 2 min). If an existing advisory fires on the current tree, add a documented
`ignore` entry in `deny.toml` with a comment and link rather than blocking the
whole brief.

### Brief 3: MSRV CI job — size XS

**Goal**: catch accidental use of post-1.95 APIs.

**Do**: add a job `msrv` to `.github/workflows/ci.yml`: ubuntu-24.04,
`dtolnay/rust-toolchain@1.95`, `Swatinem/rust-cache@v2`, then
`cargo check --workspace`. It needs the same Linux system-dependency steps as the
existing `lint` job (dav1d, nasm, cmake, libheif-from-source with the shared
cache key) because `squish-core` links native libs even for `check`. Copy the
libheif steps verbatim from the `lint` job so the cache key matches.

**Acceptance**: job green on `main`; uses toolchain 1.95 exactly (verify in the
job log).

### Brief 4: Compression-ratio regression guard — size S

**Goal**: a dependency bump that quietly worsens output (the v0.3.3 usvg
regression) must fail CI.

**Do**: add a test file `crates/squish-cli/tests/ratio_guard.rs` (or a module in
`cli_tests.rs`) that, for each image fixture in
`crates/squish-core/tests/fixtures/`, runs the binary with default settings into
a temp dir and asserts `output_size <= input_size * (1 - threshold)`.

1. First run each fixture locally to measure the *current* ratio.
2. Set each threshold ~10 percentage points looser than measured (e.g. measured
   −77% → assert ≥ 65% reduction) so encoder-version noise doesn't flake.
3. Table-drive it: `&[("sample.png", 0.65), ("sample.webp", 0.30), ...]`.
4. Always assert the universal floor `output <= input` for every fixture,
   including SVG.
5. Skip GIF/HEIC fixtures when `gifsicle`/system libs are absent — follow how
   existing tests in `crates/squish-core/tests/round_trip.rs` gate on optional
   tools.

**Acceptance**: test passes on current `main`; artificially loosening a quality
default (e.g. hard-coding quality 100) makes it fail.

**Pitfalls**: keep video/audio out of scope — ffmpeg version variance across CI
runners makes byte thresholds flaky. Image encoders are vendored Rust crates and
deterministic enough.

### Brief 5: Shell completions — size S

**Goal**: `squish completions <bash|zsh|fish>` prints a completion script; zsh is
the priority target.

**Do**:
1. Add `clap_complete = "4"` to `crates/squish-cli/Cargo.toml`.
2. Add a `Completions { shell: clap_complete::Shell }` variant to `Command` in
   `crates/squish-cli/src/cli.rs` (doc comment: "Generate a shell completion
   script (writes to stdout)").
3. Handle it in `real_main()` in `crates/squish-cli/src/main.rs` next to the
   existing subcommand matches: `clap_complete::generate(shell,
   &mut cli::Args::command(), "squish", &mut io::stdout())` — note
   `Args::command()` needs `use clap::CommandFactory`.
4. README: add an "Shell completions" subsection under Install with eval/install
   hints for zsh (`squish completions zsh > "${fpath[1]}/_squish"`), bash, fish.
5. Update `scripts/release-tap.sh` / note in its output that the tap formula
   should add `generate_completions_from_executable(bin/"squish",
   "completions")` (the formula itself lives in the separate `homebrew-tap`
   repo — just leave a clear TODO comment in the release script if the formula
   can't be edited from this repo).

**Acceptance**: integration test in `cli_tests.rs` asserting
`squish completions zsh` exits 0 and stdout contains `#compdef squish`; same for
bash (`complete -F`) and fish.

### Brief 6: Man page — size S

**Goal**: ship `squish.1`.

**Do**: prefer the hidden-subcommand approach over a build script (keeps build
simple):
1. Add `clap_mangen = "0.2"` to `crates/squish-cli/Cargo.toml`.
2. Add a `#[command(hide = true)]` subcommand `Man` that renders
   `clap_mangen::Man::new(cli::Args::command())` to stdout.
3. In `.github/workflows/release.yml`, after building, run `squish man >
   squish.1` and include `squish.1` in each release tarball.
4. Leave a note for the tap formula: `man1.install "squish.1"`.

**Acceptance**: `squish man | head -1` starts with `.TH` (roff header);
integration test asserts exit 0 + `.TH` present; release workflow change is
syntactically valid (run `actionlint` if available, else careful review).

### Brief 7: `cargo binstall` support — size XS

**Goal**: `cargo binstall squish-media-cli` fetches the prebuilt release binary
instead of compiling.

**Do**: in `crates/squish-cli/Cargo.toml` add:

```toml
[package.metadata.binstall]
pkg-url = "{ repo }/releases/download/v{ version }/squish-v{ version }-{ target }.tar.gz"
bin-dir = "squish-v{ version }-{ target }/{ bin }{ binary-ext }"
pkg-fmt = "tgz"
```

**First verify the real artifact layout**: download one asset from the latest
GitHub release and check the exact tarball name and inner directory structure,
then match the template to reality (the templates above are a guess — the actual
naming in `.github/workflows/release.yml` is the source of truth). Also confirm
which `{ target }` triples are published and note unsupported ones.

**Acceptance**: `cargo binstall squish-media-cli --dry-run` resolves a URL that
returns HTTP 200 for the current release (test with `curl -sIL <url>`).

---

## Phase 2 — Scriptability & CI story

Brief 9 depends on Brief 8. Brief 10 is independent.

### Brief 8: `--json` output mode — size M

**Goal**: machine-readable run summary for scripting and CI.

**Spec**:
- New flag `--json` in `crates/squish-cli/src/cli.rs`. Conflicts with
  `--verbose`, `--quiet`, `--watch`, and the `--stats` report (or: make
  `--stats --json` emit the stats report as JSON too — preferred if cheap, since
  `stats.rs` already has structured data).
- With `--json`: suppress all normal stdout output (progress bars from
  `indicatif` must go to stderr or be disabled; check current behavior), and at
  end of run print a single JSON document to stdout:

```json
{
  "version": 1,
  "files": [
    {
      "input": "photos/dog.png",
      "output": "photos/dog_squished.png",
      "kind": "image",
      "format": "png",
      "bytes_in": 1048576,
      "bytes_out": 245760,
      "saving_pct": 76.6,
      "status": "squished"
    }
  ],
  "totals": { "files": 8, "bytes_in": 0, "bytes_out": 0, "saving_pct": 0.0,
              "by_kind": { "image": { "files": 5, "bytes_in": 0, "bytes_out": 0 } } },
  "errors": [ { "input": "clip.mov", "message": "ffmpeg not found" } ]
}
```

- `status` is one of `squished | skipped | error`. Include a stable `"version": 1`
  field for forward compatibility.
- Exit codes unchanged (the existing `RunReport::exit_code()` logic in
  `crates/squish-cli/src/runner.rs`).
- Errors still go to stderr as text *in addition to* the `errors` array.

**Where**: `RunReport` and per-file results already exist in
`crates/squish-cli/src/runner.rs` (`RunReport`, `input_bytes()`,
`output_bytes()`); derive `serde::Serialize` on the relevant structs or build a
dedicated serializable view struct next to them. `serde_json` is already a
dependency. Rendering currently happens in `main.rs`/`runner.rs` — find where the
human summary line ("Squished 8 files …") is printed and branch there.

**Tests**: integration tests — run on a fixture dir with `--json`, parse stdout
with `serde_json`, assert schema fields, totals add up, `--dry-run --json` works,
and a batch with one failing file (e.g. a corrupt image fixture) yields an
`errors` entry plus the right exit code. Assert stdout is *only* JSON (parseable
from first byte to last).

**Docs**: README Flags block + a short "Scripting" subsection with a `jq`
example. CHANGELOG entry.

### Brief 9: GitHub Action savings summary — size S (depends on Brief 8)

**Goal**: workflows using the bundled action get a visible "saved X MB" result.

**Do** (in `action.yml`, which is a composite action):
1. Run squish with `--json`, tee the JSON to a file.
2. Parse with `jq` and append a Markdown table (files, before, after, % saved —
   totals plus per-kind) to `$GITHUB_STEP_SUMMARY`.
3. Expose outputs: `bytes-saved`, `saving-pct`, `files-squished`, and `json`
   (path to the full report) via `$GITHUB_OUTPUT`, so users can build their own
   PR comment step. Do **not** post PR comments from the action itself (needs
   token/permissions decisions best left to the consumer — document a
   copy-paste example using `peter-evans/create-or-update-comment` in the README
   instead).
4. Make the summary tolerate `args` that already contain `--json` (dedupe) and
   `--dry-run`.

**Acceptance**: `.github/workflows/action-test.yml` extended to assert the
outputs are set and the step summary file is non-empty. README GitHub Action
section documents the new outputs + PR-comment recipe.

### Brief 10: `--exclude` globs and ignore-aware walking — size M

**Goal**: skip files/dirs by pattern; big directory runs stop tripping over
`node_modules`, `.git`, build output.

**Spec**:
- New repeatable flag `--exclude <GLOB>` (e.g. `--exclude "*.min.js"
  --exclude "vendor/**"`). Globs match relative to each input path root.
- New flag `--gitignore` to also respect `.gitignore` files during recursive
  walks (opt-in — default behavior unchanged, to avoid surprising existing
  users; revisit the default at 1.0).
- Always-on built-in skip list for directory recursion: `.git`,
  `node_modules`, `target` — with a `--no-default-excludes` escape hatch.
  (Verify first whether `walker.rs` already skips hidden dirs; align, don't
  duplicate.)
- Config file support: `exclude = ["…"]` (array) in `squish.toml`, merged
  CLI-over-config like other keys in `crates/squish-cli/src/config.rs`.

**Where**: `crates/squish-cli/src/walker.rs` currently uses `walkdir`. Swap to
the `ignore` crate (same author as ripgrep; provides gitignore semantics +
custom overrides via `ignore::overrides::OverrideBuilder`) — it's the standard
tool for exactly this. Keep the walker's public API stable; watch mode
(`watch.rs`) filters events through the same predicate — make the
exclusion check a shared function both call.

**Tests**: walker unit tests (excluded glob skipped, non-recursive unaffected,
explicit file args are *never* excluded even if they match a glob — explicit
wins); integration test with a `.gitignore` + `--gitignore`; config-file
`exclude` test; watch-mode exclusion test if cheap, else skip.

**Docs**: README Flags + Config file sections; CHANGELOG.

---

## Phase 3 — Features

Independent of each other; Brief 13 is easiest since preset plumbing exists.

### Brief 11: Image metadata control (`--strip-metadata`) — size M

**Goal**: explicit, documented control over EXIF/XMP/ICC handling in images —
both a privacy feature (strip GPS) and a correctness one (orientation).

**Step 0 — investigate and document current behavior** (do this first; the
result shapes the rest): for each encoder in
`crates/squish-core/src/formats/`, determine what currently happens to
metadata. Write a fixture-based test that embeds EXIF (including an
`Orientation` tag) in a JPEG and checks the output. Likely current state:
mozjpeg/oxipng/ravif/webp re-encodes drop most metadata silently, and an
EXIF-rotated JPEG may come out *visually rotated wrong* if orientation is
neither applied nor preserved. **If an orientation bug is found, fix it as its
own PR first** (auto-orient pixels before encoding, standard practice).

**Spec** (after step 0):
- Default: strip metadata *except* apply orientation and preserve ICC profile
  (colour correctness). This matches user expectation for an optimiser.
- `--keep-metadata`: preserve EXIF/XMP/ICC where format supports it.
- Document defaults per format in README (Formats table gets a Metadata column).
- Mirror the existing `--strip-tags` audio flag naming; consider unifying
  language in help text ("audio: --strip-tags; images strip by default,
  --keep-metadata to preserve").

**Where**: `crates/squish-core/src/formats/{jpeg,png,webp,avif,heic,tiff}.rs`,
options in `crates/squish-core/src/options.rs`, flag in
`crates/squish-cli/src/cli.rs`, wiring in `main.rs`. The `img-parts` or
`kamadak-exif` crates handle EXIF read/write; mozjpeg supports marker
copy.

**Tests**: fixture with EXIF GPS + orientation; assert default output has no GPS
but correct visual orientation and retained ICC; assert `--keep-metadata` round-
trips EXIF.

### Brief 12: Never-grow guarantee — size S/M

**Goal**: squish must never write an output larger than its input, for any
format. (A size guard already exists for SVG only — see the v0.3.3 CHANGELOG
entry.)

**Spec**:
- After encoding, if `bytes_out >= bytes_in` **and** no transformation was
  requested that legitimately changes representation (`--format` conversion,
  resize, codec change): discard the encode, copy the input through as the
  output (or, with `-o`, leave the original untouched), and count the file as
  `skipped (already optimal)` in the summary and in `--json` `status`.
- When a conversion *was* requested, growth is allowed (converting a tiny PNG
  icon to AVIF can grow it) but print a per-file warning in verbose mode.
- Applies uniformly: images, video, audio, code.

**Where**: implement centrally in the per-file completion path in
`crates/squish-cli/src/runner.rs`, not per-encoder — the runner already knows
input/output sizes. Grep first for existing partial guards (SVG one lives in
`squish-core`) and remove/keep them consistently — the SVG guard can stay as an
inner optimisation but must not conflict with the unified `skipped` accounting.

**Tests**: feed an already-optimised fixture (e.g. run squish twice; second run
must report `skipped`, byte-identical output, exit 0). Integration test for the
`--format` exception path.

### Brief 13: More presets (`email`, `social`) — size S per preset

**Goal**: extend `--preset` beyond `web` with two commonly needed destinations.
The plumbing (`Preset` enum in `crates/squish-cli/src/cli.rs`,
`apply_preset` in `crates/squish-cli/src/preset.rs`, precedence: explicit flags
> preset > config) already exists — each preset is mostly a table entry + tests
+ docs.

**Spec**:
- `--preset email`: attachments that fit mail limits — images max-width 1600,
  JPEG (broadest client support) quality 80; video/audio `--target-size 20M`
  (Gmail/Outlook attachment ceiling is 20–25 MB).
- `--preset social`: media for chat/social upload limits — images max-width
  2048, WebP quality 80; video H.264 (universal playback) `--target-size 8M`
  (Discord free-tier limit, the common case people ask for).
- Same leniency rule as `web`: a preset key applies only to kinds present in
  the batch; never errors on missing kinds (there is an existing test for this
  — extend it).
- Careful with rate-control interaction: `web` uses `--quality auto`, these use
  `target-size` for video — reuse the existing "explicit rate flag beats preset"
  precedence exactly as `apply_preset` does today.

**Tests**: mirror the existing `web` preset tests (see `feat/preset-web`
commits: enum test, codec-override test, leniency test) per new preset.

**Docs**: README Flags (`--preset <web|email|social>`) + a short table of what
each preset sets; CHANGELOG.

### Brief 14: Incremental mode (`--changed-only`) — size M/L

**Goal**: re-running squish on a big directory skips files already squished with
the same settings — makes `--watch` startup, repeat runs, and the CI action
dramatically cheaper.

**Spec**:
- New flag `--changed-only`: consult a cache manifest; skip any input whose
  (content hash, effective options fingerprint) pair already produced an output
  that still exists on disk.
- Manifest: one JSON-lines file at the platform cache dir
  (`~/Library/Caches/squish/manifest.jsonl` on macOS, XDG cache on Linux) — same
  pattern as the stats ledger in `crates/squish-cli/src/stats.rs` (atomic
  append, `v` version field). Key: xxhash or blake3 of file contents + a hash of
  the resolved options struct. Prune entries opportunistically when the file
  exceeds ~1 MB.
- Cache misses behave exactly as today. `--force` bypasses the cache.
  `--dry-run` reports what would be skipped.
- Skipped-via-cache files count as `skipped` in summary/`--json`.

**Where**: gate in `crates/squish-cli/src/runner.rs` before dispatching a file;
new module `crates/squish-cli/src/cache.rs` modelled on `stats.rs`. Add `blake3`
(or `xxhash-rust`) to the CLI crate only.

**Tests**: run twice with `--changed-only` — second run squishes 0 files and is
observably fast; touching a file's *contents* (not just mtime) re-squishes it;
changing options (e.g. different `--quality`) re-squishes; deleting the output
re-squishes.

**Pitfalls**: hash file contents, not mtimes (CI checkouts have fresh mtimes —
mtime-based caching would be useless for the Action, which is the biggest
beneficiary). Document that the manifest is a cache, safe to delete.

---

## Phase 4 — Encoding quality

### Brief 15: Two-pass video encoding for `--target-size` — size M

**Goal**: more accurate size targeting; replace the current
single-pass-ABR-with-up-to-3-retries loop as the primary strategy.

**Spec**:
- For codecs/containers where ffmpeg two-pass is well supported (H.264/H.265 via
  `-pass 1/2 -passlogfile <tmp>`, VP9 via its own two-pass), run pass 1 to
  null output, then pass 2 to the real target, with the same bitrate math that
  exists today (audio bytes subtracted, container headroom).
- Keep the existing retry loop as a fallback: (a) for codecs where two-pass is
  awkward (AV1/SVT flavors vary), and (b) as an overshoot backstop after pass 2
  (should rarely trigger).
- Pass-log temp files go in a `tempfile::tempdir()`, never the source dir.

**Where**: `crates/squish-video/src/ffmpeg.rs` (encode invocation) and the
target-size math in `crates/squish-cli/src/target_size.rs` /
`crates/squish-video/src/options.rs`. Read both before starting — the retry
loop and VBV constraints described in the v0.4.0 CHANGELOG entry live there.

**Tests**: extend `crates/squish-video/tests/round_trip.rs`: fixture with
`--target-size` must land under budget; assert two ffmpeg invocations occur for
h264/h265 (the ffmpeg wrapper in `squish-media` is the seam to observe — add a
test hook or inspect via a recorded command log if one exists; otherwise assert
on outcome only: under budget within 5%).

**Pitfalls**: `-pass` requires consistent settings across both invocations;
`-an`/audio-copy interacts with pass 1 (`-an` on pass 1 is standard). Guard
Windows-style NUL vs `/dev/null` for pass-1 output via `-f null -` which is
portable.

### Brief 16: Unwind the `=` dependency pins — size XS (BLOCKED)

Blocked until `oxvg_optimiser` publishes a release past 0.0.5 that drops the
unconditional `lightningcss` `grid` feature. When Dependabot (Brief 1) surfaces
a new oxvg version: remove the `=` from `lightningcss` and `minify-html` in
`crates/squish-code/Cargo.toml`, `cargo update`, run the full test suite plus
the ratio guard (Brief 4), and check the SVG fixture output size hasn't
regressed (that was the original v0.3.3 failure mode).

### Brief 17 (stretch): `--quality auto` for video — size L

Extend perceptual auto-quality to video: search CRF for the highest value whose
VMAF (via `ffmpeg -filter:v libvmaf`, if the system ffmpeg has it — feature-detect
in `squish doctor` first) stays ≥ a visually-lossless threshold (~95), sampling a
few short segments rather than scoring full encodes. Large; spec it properly in
its own brief before starting — this entry is a placeholder so the idea isn't
lost. Prerequisite: Brief 15 (shared encode-invocation refactor).

---

## Explicitly deferred (unchanged from ROADMAP.md)

- **Windows support** — binaries, CI, Scoop. Revisit on demand.
- **Docker image** — the GitHub Action + prebuilt binaries cover CI use.
- **New input kinds (PDF, fonts)** — PDF compression would drag in
  ghostscript/qpdf as system deps for a niche win; WOFF2 fonts are usually
  already optimal. Park both.

## Suggested release mapping

| Release | Contents |
|---|---|
| v0.7.x (patch/minor) | Phase 1 — Briefs 1–7 |
| v0.8.0 | Phase 2 — Briefs 8–10 |
| v0.9.0 | Phase 3 — Briefs 11–14 |
| v1.0.0 | Phase 4 (15, 16 when unblocked) + stability pass |
