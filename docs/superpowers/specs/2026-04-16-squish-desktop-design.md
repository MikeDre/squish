# squish-desktop — Design Spec

**Date:** 2026-04-16
**Status:** Approved, pending implementation plan

## Overview

`squish-desktop` is a Tauri desktop app that wraps `squish-core` to provide drag-and-drop image compression. It lives in a separate repo from the CLI, consuming `squish-core` as a git dependency. The app is intentionally simple: drop files, watch them compress, see results.

## Goals

- Frictionless drag-and-drop compression with zero configuration required.
- Per-file progress feedback during batch processing.
- Results display with before/after sizes and savings percentages.
- Inline settings panel for power users (quality, format, lossless).
- Same compression quality as the CLI — same `squish-core` library underneath.

## Non-Goals (v1)

- File picker / "Open File" dialog (drag-and-drop only for v1).
- System tray / menu bar integration.
- Auto-update mechanism.
- Multiple simultaneous batches.
- Undo / restore original functionality (originals are never touched — same guarantee as CLI).
- Custom themes or appearance settings.
- Pre-built binary distribution / code signing (build-from-source for v1).

## Tech Stack

| Layer | Choice | Reason |
|---|---|---|
| Desktop framework | Tauri 2 | Rust backend, small binary, native webview |
| Frontend framework | React + TypeScript | Familiar ecosystem, component model fits the UI |
| Build tool | Vite | Tauri's default, fast HMR |
| State management | React useState/useReducer | App state is small — no Redux/Zustand needed |
| Styling | CSS Modules or plain CSS | Small app, no design system needed |
| Core library | `squish-core` (git dep) | Shared with CLI, all format logic lives here |
| Persistence | localStorage | Settings only (quality, format, lossless) |

## Architecture

### Repo structure

```
squish-desktop/
├── Cargo.toml                    # Tauri Rust backend
├── package.json                  # React frontend
├── tsconfig.json
├── vite.config.ts
├── src-tauri/
│   ├── Cargo.toml                # depends on squish-core (git), tauri
│   ├── tauri.conf.json
│   ├── src/
│   │   ├── main.rs               # Tauri entry point
│   │   └── commands.rs           # Tauri command handlers
│   └── icons/                    # App icons
├── src/
│   ├── main.tsx                  # React entry point
│   ├── App.tsx                   # Root component, state management
│   ├── App.css                   # Global styles
│   ├── components/
│   │   ├── DropZone.tsx          # Drag-and-drop file receiver
│   │   ├── DropZone.css
│   │   ├── FileList.tsx          # Per-file progress rows + results
│   │   ├── FileList.css
│   │   ├── FileRow.tsx           # Single file: progress bar → result
│   │   ├── FileRow.css
│   │   ├── SettingsPanel.tsx     # Collapsible quality/format/lossless
│   │   ├── SettingsPanel.css
│   │   └── Summary.tsx           # Batch summary bar
│   ├── types.ts                  # Shared TypeScript types
│   └── hooks/
│       └── useSquish.ts          # Tauri invoke + event listener hook
└── docs/
```

### Hard separation rule

The Tauri Rust backend (`commands.rs`) is a thin wrapper around `squish-core` — same discipline as the CLI. It contains no format logic, no naming logic, no compression decisions. It translates Tauri commands into `squish_core::squish_file` calls and emits progress events. Everything else lives in `squish-core`.

### Dependency on squish-core

The `src-tauri/Cargo.toml` depends on `squish-core` via git:

```toml
[dependencies]
squish-core = { git = "https://github.com/USER/squish.git" }
```

This avoids premature crates.io publishing. When `squish-core` stabilizes, switch to a crates.io dependency.

## Tauri Commands (Rust → Frontend contract)

### `squish_files`

```rust
#[tauri::command]
async fn squish_files(
    app: tauri::AppHandle,
    paths: Vec<String>,
    options: SquishOptionsPayload,
) -> Result<BatchResult, String>
```

**Input:**
- `paths`: absolute file/directory paths from the drop event.
- `options`: `{ quality?: number, lossless: boolean, format?: string }`.

**Behavior:**
1. Expand directories (non-recursive — desktop users drop a folder, they mean the top level; recursive can be added later).
2. For each file, emit a `squish://file-start` event.
3. Call `squish_core::squish_file` on a rayon thread pool.
4. On completion, emit a `squish://file-done` event with the result.
5. On error, emit a `squish://file-error` event.
6. After all files, return a `BatchResult` summary.

**Events emitted:**

| Event | Payload |
|---|---|
| `squish://file-start` | `{ id: string, path: string, filename: string }` |
| `squish://file-done` | `{ id: string, inputBytes: number, outputBytes: number, outputPath: string, reductionPercent: number, duration_ms: number }` |
| `squish://file-error` | `{ id: string, error: string }` |

Each file gets a unique `id` (UUID or incrementing counter) so the frontend can match events to rows.

### `get_version`

```rust
#[tauri::command]
fn get_version() -> String
```

Returns the app version string for display.

## Frontend Components

### App.tsx — Root state machine

The app has three visual states:

1. **Idle** — drop zone visible, no files. Settings panel optionally expanded.
2. **Processing** — drop zone still visible (dimmed), file list below showing per-file progress.
3. **Done** — drop zone visible, file list shows results with savings. Summary bar at top of list.

State transitions:
- Idle → Processing: files dropped.
- Processing → Done: all files complete.
- Done → Processing: new files dropped (clears previous results).
- Any → Idle: never automatically (user sees results until next drop).

State is managed with `useReducer` in App.tsx. Shape:

```typescript
type AppState = {
  status: 'idle' | 'processing' | 'done';
  files: FileEntry[];
  settings: Settings;
};

type FileEntry = {
  id: string;
  filename: string;
  path: string;
  status: 'pending' | 'compressing' | 'done' | 'error';
  inputBytes?: number;
  outputBytes?: number;
  reductionPercent?: number;
  error?: string;
};

type Settings = {
  quality: number | null;   // null = format default
  lossless: boolean;
  format: string | null;    // null = preserve input format
};
```

### DropZone.tsx

- Full-width area at the top of the window.
- Visual states: default, drag-hover (highlighted border/background), processing (dimmed but still accepts drops — queuing is a v2 concern, dropping during processing is ignored in v1).
- On drop: reads file paths from the Tauri drop event, dispatches to the `useSquish` hook.
- Accepts files and folders.

### FileList.tsx + FileRow.tsx

- Appears below the drop zone when files are being processed or results are shown.
- Each `FileRow` shows:
  - **Pending/compressing**: filename + animated progress bar.
  - **Done**: filename, input size → output size, savings %, small bar visualization.
  - **Error**: filename + error message in red.
- Rows appear immediately on drop (status: pending), transition to compressing when `file-start` fires, then to done/error.

### SettingsPanel.tsx

- Collapsible panel, toggled by a gear icon.
- Controls:
  - **Quality slider**: 0–100, with "Auto" default (sends null → format default).
  - **Format dropdown**: Auto (preserve input), PNG, JPEG, WebP, AVIF, SVG, GIF, HEIC.
  - **Lossless toggle**: checkbox.
- Settings persist to `localStorage` on change, loaded on app start.
- Changes take effect on the next drop (don't affect in-progress batches).

### Summary.tsx

- Appears at the top of the file list when batch completes.
- Shows: total files, total input size → output size, overall savings %, total time.
- Same format as the CLI: `Squished 47 files · 38.2 MB → 11.6 MB (-69.6%) · 3.2s`

### useSquish.ts — Tauri bridge hook

Custom React hook that:
1. Calls `invoke("squish_files", { paths, options })` on the Tauri backend.
2. Listens for `squish://file-start`, `squish://file-done`, `squish://file-error` events.
3. Dispatches state updates to the App reducer.
4. Cleans up event listeners on unmount.

## Window Configuration

- **Title**: "squish"
- **Default size**: 500 x 600 px (portrait, feels like a utility)
- **Min size**: 400 x 400 px
- **Resizable**: yes
- **Decorations**: default OS chrome (title bar, close/minimize/maximize)
- **File drop**: enabled via Tauri's `fileDropEnabled` config

## Error Handling

- **Per-file errors** don't abort the batch — same as CLI. The row shows the error, other files continue.
- **Fatal errors** (e.g., can't read any paths): shown as an inline error message in the drop zone area.
- **Missing dependency** (gifsicle for GIF, libheif for HEIC): the error message from `squish-core` includes install hints, shown in the file row.

## Testing

### Rust (src-tauri)

- Unit tests for command handler logic (mock the Tauri app handle or test the inner functions directly).
- The actual compression is tested in `squish-core` — no need to duplicate format tests here.

### Frontend

- Component tests with React Testing Library for DropZone, FileList, SettingsPanel.
- Integration test: mock Tauri invoke/events, simulate a drop → verify rows appear → mock file-done events → verify results display.

### Manual

- Drop a PNG → verify `_squished.png` appears, row shows savings.
- Drop a folder → verify all recognized files are compressed.
- Drop during processing → verify ignored (not queued).
- Change settings → drop file → verify settings are applied.
- Drop a corrupt file + a good file → verify good file succeeds, corrupt shows error.

## Distribution (v1)

- Build from source: `npm install && npm run tauri build`.
- No pre-built binaries, code signing, or auto-update in v1.
- System dependencies same as CLI: gifsicle for GIF, libheif + x265 for HEIC.

## Future Work (not v1)

- File picker button as alternative to drag-and-drop.
- Recursive folder option (toggle in settings).
- Batch queuing (drop new files while processing).
- System tray with "quick squish" drop target.
- Pre-built binaries + code signing + auto-update.
- "Reveal in Finder/Explorer" button per result row.
- Dark mode / theme support.
