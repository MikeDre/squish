# squish-desktop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Tauri 2 desktop app (`squish-desktop`) that wraps `squish-core` for drag-and-drop image compression — separate repo from the CLI, same compression library underneath.

**Architecture:** New repo at `/Users/michaelandrejoda/Sites/squish-desktop`. Tauri 2 backend (Rust) exposes two commands: `get_version` and `squish_files`. The `squish_files` command receives paths + options, expands directories, calls `squish_core::squish_file` per file on a rayon thread pool, and emits per-file progress events via Tauri's event system. React + TypeScript frontend with Vite. Five components: DropZone, FileRow, FileList, SettingsPanel, Summary. State managed with `useReducer` in App.tsx. Custom `useSquish` hook bridges Tauri invoke/events to React state.

**Tech Stack:**
- **Desktop framework:** Tauri 2
- **Frontend:** React 18+ with TypeScript, Vite
- **Core library:** `squish-core` (git dependency from `https://github.com/MikeDre/squish.git`)
- **Rust deps:** tauri 2, serde, rayon, walkdir
- **Test:** Vitest + React Testing Library (frontend), `cargo test` (Rust)

**Spec:** `/Users/michaelandrejoda/Sites/squish/docs/superpowers/specs/2026-04-16-squish-desktop-design.md`

**Repo root:** `/Users/michaelandrejoda/Sites/squish-desktop` (new repo, created in Task 1)

**squish-core API surface (for reference):**
```rust
// squish_core::SquishOptions
pub struct SquishOptions {
    pub quality: Option<u8>,
    pub lossless: bool,
    pub output_format: Option<Format>,
    pub force_overwrite: bool,
}

// squish_core::Format — does NOT derive Serialize/Deserialize
pub enum Format { Png, Jpeg, Webp, Avif, Svg, Gif, Heic, Tiff }
impl Format {
    pub fn extension(&self) -> &'static str;
    pub fn parse(s: &str) -> Option<Format>;
}

// squish_core::SquishResult
pub struct SquishResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub format_in: Format,
    pub format_out: Format,
    pub duration: Duration,
}
impl SquishResult {
    pub fn reduction_percent(&self) -> f64;
}

// squish_core::SquishError — thiserror enum
pub enum SquishError {
    UnsupportedFormat { path, reason },
    DecodeFailed { path, source },
    EncodeFailed { path, source },
    Io(std::io::Error),
    MissingDependency { name, install_hint },
}

// Main entry point
pub fn squish_file(input: &Path, opts: &SquishOptions) -> Result<SquishResult, SquishError>;
pub fn detect_format(path: &Path, head: &[u8]) -> Option<Format>;
```

---

## Prerequisites

Before executing any task:

1. **Upgrade Node.js to 18+.** Current version is v16.0.0, which is too old for Tauri 2 + Vite.
   ```bash
   nvm install 20
   nvm use 20
   ```
2. **Install Tauri CLI:**
   ```bash
   cargo install tauri-cli --version "^2"
   ```
3. **Verify Rust toolchain:** `rustc --version` should be 1.77+ (current is 1.93.0 — fine).
4. **System deps:** same as CLI — `gifsicle`, `libheif`, `x265` already installed from CLI work.

---

## File Map

### Created by this plan

```
squish-desktop/
├── .gitignore
├── package.json
├── tsconfig.json
├── tsconfig.node.json
├── vite.config.ts
├── index.html
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── icons/                         # Tauri default icons (scaffolded)
│   └── src/
│       ├── main.rs                    # Tauri entry point + command registration
│       └── commands.rs                # squish_files + get_version commands
├── src/
│   ├── main.tsx                       # React DOM entry point
│   ├── App.tsx                        # Root component, useReducer state machine
│   ├── App.css                        # Global + layout styles
│   ├── types.ts                       # Shared TypeScript types (FileEntry, Settings, etc.)
│   ├── components/
│   │   ├── DropZone.tsx               # Drag-and-drop file receiver
│   │   ├── DropZone.css
│   │   ├── FileRow.tsx                # Single file: progress → result
│   │   ├── FileRow.css
│   │   ├── FileList.tsx               # Scrollable list of FileRows
│   │   ├── FileList.css
│   │   ├── SettingsPanel.tsx          # Collapsible quality/format/lossless
│   │   ├── SettingsPanel.css
│   │   └── Summary.tsx                # Batch summary bar
│   └── hooks/
│       └── useSquish.ts               # Tauri invoke + event listener bridge
└── src/__tests__/
    ├── App.test.tsx
    ├── DropZone.test.tsx
    ├── FileRow.test.tsx
    ├── FileList.test.tsx
    ├── SettingsPanel.test.tsx
    └── useSquish.test.tsx
```

### Module responsibilities

- **`commands.rs`** — thin wrapper. Converts frontend JSON payloads to `squish_core` types, runs compression on rayon, emits Tauri events. No format logic.
- **`main.rs`** — Tauri builder with command registration. Nothing else.
- **`types.ts`** — single source of truth for `FileEntry`, `Settings`, `AppState`, event payload types.
- **`useSquish.ts`** — calls `invoke("squish_files")`, subscribes to Tauri events, dispatches to the App reducer. Owns all Tauri IPC.
- **`App.tsx`** — `useReducer` state machine (`idle` → `processing` → `done`), composes all components.
- **`DropZone.tsx`** — handles drag/drop events, calls `useSquish` to start compression.
- **`FileRow.tsx`** — renders one file's state (pending/compressing/done/error).
- **`FileList.tsx`** — maps `FileEntry[]` to `FileRow` components.
- **`SettingsPanel.tsx`** — collapsible settings, persists to localStorage.
- **`Summary.tsx`** — batch total summary bar.

---

## Task 1: Scaffold Tauri + React + TypeScript project

**Files:**
- Create: entire `squish-desktop/` directory via `create-tauri-app`

- [ ] **Step 1.1: Create the project with create-tauri-app**

```bash
cd /Users/michaelandrejoda/Sites
npm create tauri-app@latest squish-desktop -- --template react-ts --manager npm
```

When prompted:
- Project name: `squish-desktop`
- Package manager: `npm`
- UI template: `React`
- UI flavor: `TypeScript`

- [ ] **Step 1.2: Initialize git repo**

```bash
cd /Users/michaelandrejoda/Sites/squish-desktop
git init
```

- [ ] **Step 1.3: Verify scaffold builds**

```bash
cd /Users/michaelandrejoda/Sites/squish-desktop
npm install
cargo tauri dev
```

Expected: a window opens with the default Tauri + React template. Close it.

- [ ] **Step 1.4: Clean up scaffold boilerplate**

Remove the default template content. Replace `src/App.tsx`:

```tsx
function App() {
  return (
    <div className="app">
      <h1>squish</h1>
      <p>Drop images here to compress them.</p>
    </div>
  );
}

export default App;
```

Replace `src/App.css`:

```css
.app {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 100vh;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}
```

Delete any extra scaffold files that aren't needed (e.g., `assets/`, default logos, `src/styles.css` if it exists). Keep `index.html`, `main.tsx`, `vite.config.ts`, `tsconfig.json`, `tsconfig.node.json`.

- [ ] **Step 1.5: Update window config**

Modify `src-tauri/tauri.conf.json` — find the `"windows"` array and update:

```json
{
  "label": "main",
  "title": "squish",
  "width": 500,
  "height": 600,
  "minWidth": 400,
  "minHeight": 400,
  "resizable": true
}
```

Also update the top-level `"productName"` to `"squish"` and `"identifier"` to `"com.squish.desktop"`.

- [ ] **Step 1.6: Verify clean build**

```bash
npm run build
cargo tauri build --debug
```

Expected: builds without errors.

- [ ] **Step 1.7: Commit**

```bash
git add -A
git commit -m "scaffold: Tauri 2 + React + TypeScript via create-tauri-app"
```

---

## Task 2: Wire squish-core dependency + Tauri commands

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 2.1: Add dependencies to src-tauri/Cargo.toml**

Add these to the existing `[dependencies]` section in `src-tauri/Cargo.toml`:

```toml
squish-core = { git = "https://github.com/MikeDre/squish.git", branch = "main" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rayon = "1"
walkdir = "2"
```

Ensure `tauri` already has the `"unstable"` feature or at minimum the default features from the scaffold. The scaffold should have `tauri` and `tauri-build` already.

- [ ] **Step 2.2: Create commands.rs with serde types + get_version**

Create `src-tauri/src/commands.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Deserialize)]
pub struct SquishOptionsPayload {
    pub quality: Option<u8>,
    pub lossless: bool,
    pub format: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct FileStartEvent {
    pub id: String,
    pub path: String,
    pub filename: String,
}

#[derive(Serialize, Clone)]
pub struct FileDoneEvent {
    pub id: String,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub output_path: String,
    pub reduction_percent: f64,
    pub duration_ms: u64,
}

#[derive(Serialize, Clone)]
pub struct FileErrorEvent {
    pub id: String,
    pub error: String,
}

#[derive(Serialize)]
pub struct BatchResult {
    pub total_files: usize,
    pub success_count: usize,
    pub error_count: usize,
    pub skipped_count: usize,
    pub total_input_bytes: u64,
    pub total_output_bytes: u64,
    pub total_duration_ms: u64,
}

#[tauri::command]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Expand paths: files pass through, directories are walked (top-level only).
pub fn expand_paths(paths: &[String]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for p in paths {
        let path = PathBuf::from(p);
        if path.is_file() {
            files.push(path);
        } else if path.is_dir() {
            let walker = WalkDir::new(&path)
                .follow_links(false)
                .max_depth(1);
            for entry in walker.into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    files.push(entry.into_path());
                }
            }
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_version_returns_something() {
        let v = get_version();
        assert!(!v.is_empty());
    }

    #[test]
    fn expand_paths_with_nonexistent_path_returns_empty() {
        let result = expand_paths(&["/nonexistent/path/xyz".into()]);
        assert!(result.is_empty());
    }
}
```

- [ ] **Step 2.3: Wire get_version into main.rs**

Replace `src-tauri/src/main.rs`:

```rust
mod commands;

#[cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 2.4: Verify it compiles**

```bash
cd /Users/michaelandrejoda/Sites/squish-desktop
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: `get_version_returns_something` and `expand_paths_with_nonexistent_path_returns_empty` pass.

- [ ] **Step 2.5: Commit**

```bash
git add src-tauri
git commit -m "feat: squish-core git dep + get_version command + expand_paths"
```

---

## Task 3: squish_files Tauri command with event emission

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 3.1: Implement squish_files command**

Add to the bottom of `src-tauri/src/commands.rs` (before the `#[cfg(test)]` block):

```rust
use squish_core::{squish_file, Format, SquishOptions};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;
use tauri::{AppHandle, Emitter};

fn to_squish_options(payload: &SquishOptionsPayload) -> SquishOptions {
    SquishOptions {
        quality: payload.quality,
        lossless: payload.lossless,
        output_format: payload.format.as_deref().and_then(Format::parse),
        force_overwrite: false,
    }
}

/// Peek at the first 32 bytes to detect format. Returns None for unrecognized files.
fn peek_format(path: &PathBuf) -> Option<squish_core::Format> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 32];
    let n = f.read(&mut head).ok()?;
    squish_core::detect_format(path, &head[..n])
}

#[tauri::command]
pub async fn squish_files(
    app: AppHandle,
    paths: Vec<String>,
    options: SquishOptionsPayload,
) -> Result<BatchResult, String> {
    let opts = to_squish_options(&options);
    let all_files = expand_paths(&paths);

    // Partition into known-format and skipped.
    let mut known: Vec<PathBuf> = Vec::new();
    let mut skipped_count: usize = 0;
    for path in &all_files {
        if peek_format(path).is_some() {
            known.push(path.clone());
        } else {
            skipped_count += 1;
        }
    }

    let start = Instant::now();
    let success_count = AtomicUsize::new(0);
    let error_count = AtomicUsize::new(0);
    let total_input = AtomicU64::new(0);
    let total_output = AtomicU64::new(0);

    // Emit file-start for all known files, assign IDs.
    let work_items: Vec<(String, PathBuf)> = known
        .into_iter()
        .enumerate()
        .map(|(i, path)| {
            let id = format!("file-{i}");
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            let _ = app.emit("squish://file-start", FileStartEvent {
                id: id.clone(),
                path: path.display().to_string(),
                filename,
            });

            (id, path)
        })
        .collect();

    // Process in parallel with rayon.
    let results: Vec<(String, Result<squish_core::SquishResult, squish_core::SquishError>)> =
        work_items
            .into_par_iter()
            .map(|(id, path)| {
                let result = squish_file(&path, &opts);
                (id, result)
            })
            .collect();

    // Emit per-file results.
    for (id, result) in results {
        match result {
            Ok(r) => {
                success_count.fetch_add(1, Ordering::SeqCst);
                total_input.fetch_add(r.input_bytes, Ordering::SeqCst);
                total_output.fetch_add(r.output_bytes, Ordering::SeqCst);

                let _ = app.emit("squish://file-done", FileDoneEvent {
                    id,
                    input_bytes: r.input_bytes,
                    output_bytes: r.output_bytes,
                    output_path: r.output_path.display().to_string(),
                    reduction_percent: r.reduction_percent(),
                    duration_ms: r.duration.as_millis() as u64,
                });
            }
            Err(e) => {
                error_count.fetch_add(1, Ordering::SeqCst);

                let _ = app.emit("squish://file-error", FileErrorEvent {
                    id,
                    error: format!("{e}"),
                });
            }
        }
    }

    Ok(BatchResult {
        total_files: all_files.len(),
        success_count: success_count.load(Ordering::SeqCst),
        error_count: error_count.load(Ordering::SeqCst),
        skipped_count,
        total_input_bytes: total_input.load(Ordering::SeqCst),
        total_output_bytes: total_output.load(Ordering::SeqCst),
        total_duration_ms: start.elapsed().as_millis() as u64,
    })
}
```

- [ ] **Step 3.2: Register squish_files in main.rs**

Update the `invoke_handler` in `src-tauri/src/main.rs`:

```rust
mod commands;

#[cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_version,
            commands::squish_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 3.3: Verify it compiles**

```bash
cargo build --manifest-path src-tauri/Cargo.toml
```

Expected: compiles without errors. (Full integration testing happens at the end with the UI wired up.)

- [ ] **Step 3.4: Commit**

```bash
git add src-tauri
git commit -m "feat: squish_files command with rayon parallelism + event emission"
```

---

## Task 4: TypeScript types

**Files:**
- Create: `src/types.ts`

- [ ] **Step 4.1: Create types.ts**

Create `src/types.ts`:

```typescript
// --- Tauri event payloads (must match Rust structs in commands.rs) ---

export interface FileStartPayload {
  id: string;
  path: string;
  filename: string;
}

export interface FileDonePayload {
  id: string;
  input_bytes: number;
  output_bytes: number;
  output_path: string;
  reduction_percent: number;
  duration_ms: number;
}

export interface FileErrorPayload {
  id: string;
  error: string;
}

export interface BatchResult {
  total_files: number;
  success_count: number;
  error_count: number;
  skipped_count: number;
  total_input_bytes: number;
  total_output_bytes: number;
  total_duration_ms: number;
}

// --- Frontend state ---

export type FileStatus = 'pending' | 'compressing' | 'done' | 'error';

export interface FileEntry {
  id: string;
  filename: string;
  path: string;
  status: FileStatus;
  inputBytes?: number;
  outputBytes?: number;
  reductionPercent?: number;
  outputPath?: string;
  durationMs?: number;
  error?: string;
}

export interface Settings {
  quality: number | null;  // null = format default
  lossless: boolean;
  format: string | null;   // null = preserve input format
}

export type AppStatus = 'idle' | 'processing' | 'done';

export interface AppState {
  status: AppStatus;
  files: FileEntry[];
  settings: Settings;
}

// --- Reducer actions ---

export type AppAction =
  | { type: 'START_BATCH' }
  | { type: 'FILE_START'; payload: FileStartPayload }
  | { type: 'FILE_DONE'; payload: FileDonePayload }
  | { type: 'FILE_ERROR'; id: string; error: string }
  | { type: 'BATCH_COMPLETE' }
  | { type: 'UPDATE_SETTINGS'; settings: Partial<Settings> };

// --- Settings defaults ---

export const DEFAULT_SETTINGS: Settings = {
  quality: null,
  lossless: false,
  format: null,
};

export const FORMAT_OPTIONS = [
  { value: '', label: 'Auto (preserve input)' },
  { value: 'png', label: 'PNG' },
  { value: 'jpg', label: 'JPEG' },
  { value: 'webp', label: 'WebP' },
  { value: 'avif', label: 'AVIF' },
  { value: 'svg', label: 'SVG' },
  { value: 'gif', label: 'GIF' },
  { value: 'heic', label: 'HEIC' },
] as const;
```

- [ ] **Step 4.2: Verify TypeScript compiles**

```bash
npx tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4.3: Commit**

```bash
git add src/types.ts
git commit -m "feat: TypeScript types for Tauri IPC + app state"
```

---

## Task 5: Set up Vitest + testing utilities

**Files:**
- Modify: `package.json`
- Modify: `vite.config.ts`
- Create: `src/__tests__/setup.ts`

- [ ] **Step 5.1: Install test dependencies**

```bash
npm install -D vitest @testing-library/react @testing-library/jest-dom @testing-library/user-event jsdom
```

- [ ] **Step 5.2: Configure Vitest in vite.config.ts**

Update `vite.config.ts` to include test config:

```typescript
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: process.env.TAURI_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/__tests__/setup.ts"],
  },
});
```

- [ ] **Step 5.3: Create test setup file with Tauri mocks**

Create `src/__tests__/setup.ts`:

```typescript
import "@testing-library/jest-dom";

// Mock Tauri API for tests running outside the Tauri runtime.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: vi.fn(() => ({
    onDragDropEvent: vi.fn(() => Promise.resolve(() => {})),
  })),
}));
```

- [ ] **Step 5.4: Add test script to package.json**

Add to the `"scripts"` section of `package.json`:

```json
"test": "vitest run",
"test:watch": "vitest"
```

- [ ] **Step 5.5: Verify test runner works**

Create a trivial test to confirm the setup. Create `src/__tests__/smoke.test.ts`:

```typescript
import { describe, it, expect } from "vitest";

describe("test setup", () => {
  it("runs", () => {
    expect(1 + 1).toBe(2);
  });
});
```

Run:

```bash
npm test
```

Expected: 1 test passes.

- [ ] **Step 5.6: Commit**

```bash
git add package.json package-lock.json vite.config.ts src/__tests__/setup.ts src/__tests__/smoke.test.ts
git commit -m "test: Vitest + React Testing Library + Tauri mocks"
```

---

## Task 6: DropZone component

**Files:**
- Create: `src/components/DropZone.tsx`
- Create: `src/components/DropZone.css`
- Create: `src/__tests__/DropZone.test.tsx`

- [ ] **Step 6.1: Write failing tests for DropZone**

Create `src/__tests__/DropZone.test.tsx`:

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { DropZone } from "../components/DropZone";

describe("DropZone", () => {
  it("renders drop prompt when idle", () => {
    render(<DropZone status="idle" onDrop={vi.fn()} />);
    expect(screen.getByText(/drop images here/i)).toBeInTheDocument();
  });

  it("shows processing state when processing", () => {
    render(<DropZone status="processing" onDrop={vi.fn()} />);
    expect(screen.getByText(/compressing/i)).toBeInTheDocument();
  });

  it("shows ready for more when done", () => {
    render(<DropZone status="done" onDrop={vi.fn()} />);
    expect(screen.getByText(/drop more/i)).toBeInTheDocument();
  });
});
```

Run: `npm test`
Expected: FAIL — `DropZone` doesn't exist.

- [ ] **Step 6.2: Implement DropZone**

Create `src/components/DropZone.tsx`:

```tsx
import { useEffect, useRef, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { AppStatus } from "../types";
import "./DropZone.css";

interface DropZoneProps {
  status: AppStatus;
  onDrop: (paths: string[]) => void;
}

export function DropZone({ status, onDrop }: DropZoneProps) {
  const [isDragOver, setIsDragOver] = useState(false);
  const unlistenRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function setupDragDrop() {
      try {
        const appWindow = getCurrentWebviewWindow();
        const unlisten = await appWindow.onDragDropEvent((event) => {
          if (cancelled) return;

          if (event.payload.type === "over") {
            setIsDragOver(true);
          } else if (event.payload.type === "drop") {
            setIsDragOver(false);
            if (status !== "processing") {
              onDrop(event.payload.paths);
            }
          } else if (event.payload.type === "leave") {
            setIsDragOver(false);
          }
        });
        unlistenRef.current = unlisten;
      } catch {
        // Outside Tauri runtime (tests) — no-op.
      }
    }

    setupDragDrop();

    return () => {
      cancelled = true;
      unlistenRef.current?.();
    };
  }, [status, onDrop]);

  const statusText = () => {
    switch (status) {
      case "idle":
        return "Drop images here to compress";
      case "processing":
        return "Compressing...";
      case "done":
        return "Drop more images to compress";
    }
  };

  const className = [
    "dropzone",
    isDragOver ? "dropzone--drag-over" : "",
    status === "processing" ? "dropzone--processing" : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div className={className}>
      <div className="dropzone__content">
        <p className="dropzone__text">{statusText()}</p>
      </div>
    </div>
  );
}
```

Create `src/components/DropZone.css`:

```css
.dropzone {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 160px;
  margin: 16px;
  border: 2px dashed #ccc;
  border-radius: 12px;
  background: #fafafa;
  transition: all 0.2s ease;
  cursor: default;
  user-select: none;
}

.dropzone--drag-over {
  border-color: #4a90d9;
  background: #eef4fc;
}

.dropzone--processing {
  opacity: 0.6;
}

.dropzone__content {
  text-align: center;
  padding: 24px;
}

.dropzone__text {
  font-size: 16px;
  color: #666;
  margin: 0;
}
```

- [ ] **Step 6.3: Run tests**

```bash
npm test
```

Expected: all 3 DropZone tests pass.

- [ ] **Step 6.4: Commit**

```bash
git add src/components/DropZone.tsx src/components/DropZone.css src/__tests__/DropZone.test.tsx
git commit -m "feat: DropZone component with drag-and-drop states"
```

---

## Task 7: FileRow component

**Files:**
- Create: `src/components/FileRow.tsx`
- Create: `src/components/FileRow.css`
- Create: `src/__tests__/FileRow.test.tsx`

- [ ] **Step 7.1: Write failing tests for FileRow**

Create `src/__tests__/FileRow.test.tsx`:

```tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { FileRow } from "../components/FileRow";
import type { FileEntry } from "../types";

const pending: FileEntry = {
  id: "1",
  filename: "photo.png",
  path: "/tmp/photo.png",
  status: "pending",
};

const compressing: FileEntry = {
  ...pending,
  status: "compressing",
};

const done: FileEntry = {
  ...pending,
  status: "done",
  inputBytes: 100_000,
  outputBytes: 30_000,
  reductionPercent: 70.0,
  durationMs: 1200,
};

const error: FileEntry = {
  ...pending,
  status: "error",
  error: "decode failed",
};

describe("FileRow", () => {
  it("shows filename in all states", () => {
    render(<FileRow file={pending} />);
    expect(screen.getByText("photo.png")).toBeInTheDocument();
  });

  it("shows progress indicator when compressing", () => {
    render(<FileRow file={compressing} />);
    expect(screen.getByRole("progressbar")).toBeInTheDocument();
  });

  it("shows sizes and savings when done", () => {
    render(<FileRow file={done} />);
    expect(screen.getByText(/97\.7 KB/)).toBeInTheDocument();
    expect(screen.getByText(/29\.3 KB/)).toBeInTheDocument();
    expect(screen.getByText(/70\.0%/)).toBeInTheDocument();
  });

  it("shows error message when failed", () => {
    render(<FileRow file={error} />);
    expect(screen.getByText(/decode failed/)).toBeInTheDocument();
  });
});
```

Run: `npm test`
Expected: FAIL — `FileRow` doesn't exist.

- [ ] **Step 7.2: Implement FileRow**

Create `src/components/FileRow.tsx`:

```tsx
import type { FileEntry } from "../types";
import "./FileRow.css";

interface FileRowProps {
  file: FileEntry;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function FileRow({ file }: FileRowProps) {
  return (
    <div className={`file-row file-row--${file.status}`}>
      <div className="file-row__name">{file.filename}</div>

      {(file.status === "pending" || file.status === "compressing") && (
        <div className="file-row__progress">
          <div
            role="progressbar"
            className="file-row__progress-bar"
            aria-label={`Compressing ${file.filename}`}
          >
            <div className="file-row__progress-fill" />
          </div>
        </div>
      )}

      {file.status === "done" && file.inputBytes != null && file.outputBytes != null && (
        <div className="file-row__result">
          <span className="file-row__sizes">
            {formatBytes(file.inputBytes)} → {formatBytes(file.outputBytes)}
          </span>
          <span className="file-row__savings">
            {file.reductionPercent != null && file.reductionPercent >= 0
              ? `-${file.reductionPercent.toFixed(1)}%`
              : `+${Math.abs(file.reductionPercent ?? 0).toFixed(1)}%`}
          </span>
        </div>
      )}

      {file.status === "error" && (
        <div className="file-row__error">{file.error}</div>
      )}
    </div>
  );
}
```

Create `src/components/FileRow.css`:

```css
.file-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 16px;
  border-bottom: 1px solid #eee;
  gap: 12px;
}

.file-row__name {
  font-size: 14px;
  font-weight: 500;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex-shrink: 1;
}

.file-row__progress {
  flex: 1;
  max-width: 120px;
}

.file-row__progress-bar {
  height: 6px;
  background: #eee;
  border-radius: 3px;
  overflow: hidden;
}

.file-row__progress-fill {
  height: 100%;
  background: #4a90d9;
  border-radius: 3px;
  animation: progress-indeterminate 1.5s ease-in-out infinite;
}

@keyframes progress-indeterminate {
  0% { width: 0%; margin-left: 0%; }
  50% { width: 60%; margin-left: 20%; }
  100% { width: 0%; margin-left: 100%; }
}

.file-row__result {
  display: flex;
  gap: 12px;
  align-items: center;
  font-size: 13px;
  color: #666;
  flex-shrink: 0;
}

.file-row__savings {
  font-weight: 600;
  color: #2a9d2a;
}

.file-row__error {
  font-size: 13px;
  color: #d32f2f;
  flex: 1;
  text-align: right;
}

.file-row--error .file-row__name {
  color: #d32f2f;
}
```

- [ ] **Step 7.3: Run tests**

```bash
npm test
```

Expected: all 4 FileRow tests pass.

- [ ] **Step 7.4: Commit**

```bash
git add src/components/FileRow.tsx src/components/FileRow.css src/__tests__/FileRow.test.tsx
git commit -m "feat: FileRow component with progress/result/error states"
```

---

## Task 8: FileList + Summary components

**Files:**
- Create: `src/components/FileList.tsx`
- Create: `src/components/FileList.css`
- Create: `src/components/Summary.tsx`
- Create: `src/__tests__/FileList.test.tsx`

- [ ] **Step 8.1: Write failing tests**

Create `src/__tests__/FileList.test.tsx`:

```tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { FileList } from "../components/FileList";
import type { FileEntry, BatchResult } from "../types";

const files: FileEntry[] = [
  {
    id: "1",
    filename: "a.png",
    path: "/a.png",
    status: "done",
    inputBytes: 100_000,
    outputBytes: 30_000,
    reductionPercent: 70.0,
    durationMs: 500,
  },
  {
    id: "2",
    filename: "b.jpg",
    path: "/b.jpg",
    status: "done",
    inputBytes: 50_000,
    outputBytes: 40_000,
    reductionPercent: 20.0,
    durationMs: 300,
  },
];

const batchResult: BatchResult = {
  total_files: 2,
  success_count: 2,
  error_count: 0,
  skipped_count: 0,
  total_input_bytes: 150_000,
  total_output_bytes: 70_000,
  total_duration_ms: 800,
};

describe("FileList", () => {
  it("renders a row for each file", () => {
    render(<FileList files={files} batchResult={null} />);
    expect(screen.getByText("a.png")).toBeInTheDocument();
    expect(screen.getByText("b.jpg")).toBeInTheDocument();
  });

  it("shows summary when batch result is provided", () => {
    render(<FileList files={files} batchResult={batchResult} />);
    expect(screen.getByText(/2 files/)).toBeInTheDocument();
  });

  it("renders empty when no files", () => {
    const { container } = render(<FileList files={[]} batchResult={null} />);
    expect(container.querySelector(".file-list")).toBeNull();
  });
});
```

Run: `npm test`
Expected: FAIL.

- [ ] **Step 8.2: Implement Summary**

Create `src/components/Summary.tsx`:

```tsx
import type { BatchResult } from "../types";

interface SummaryProps {
  result: BatchResult;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

export function Summary({ result }: SummaryProps) {
  const saved = result.total_input_bytes > 0
    ? ((1 - result.total_output_bytes / result.total_input_bytes) * 100)
    : 0;

  return (
    <div className="summary">
      <span>
        Squished {result.success_count} files
        {" · "}
        {formatBytes(result.total_input_bytes)} → {formatBytes(result.total_output_bytes)}
        {" "}
        ({saved >= 0 ? `-${saved.toFixed(1)}` : `+${Math.abs(saved).toFixed(1)}`}%)
        {" · "}
        {formatDuration(result.total_duration_ms)}
      </span>
      {result.error_count > 0 && (
        <span className="summary__errors">
          {" · "}{result.error_count} failed
        </span>
      )}
    </div>
  );
}
```

- [ ] **Step 8.3: Implement FileList**

Create `src/components/FileList.tsx`:

```tsx
import { FileRow } from "./FileRow";
import { Summary } from "./Summary";
import type { FileEntry, BatchResult } from "../types";
import "./FileList.css";

interface FileListProps {
  files: FileEntry[];
  batchResult: BatchResult | null;
}

export function FileList({ files, batchResult }: FileListProps) {
  if (files.length === 0) return null;

  return (
    <div className="file-list">
      {batchResult && <Summary result={batchResult} />}
      <div className="file-list__rows">
        {files.map((file) => (
          <FileRow key={file.id} file={file} />
        ))}
      </div>
    </div>
  );
}
```

Create `src/components/FileList.css`:

```css
.file-list {
  flex: 1;
  overflow-y: auto;
  margin: 0 16px 16px;
  border: 1px solid #eee;
  border-radius: 8px;
  background: #fff;
}

.file-list__rows {
  max-height: 400px;
  overflow-y: auto;
}

.summary {
  padding: 12px 16px;
  font-size: 13px;
  font-weight: 600;
  color: #333;
  background: #f5f5f5;
  border-bottom: 1px solid #eee;
  border-radius: 8px 8px 0 0;
}

.summary__errors {
  color: #d32f2f;
}
```

- [ ] **Step 8.4: Run tests**

```bash
npm test
```

Expected: all FileList tests pass.

- [ ] **Step 8.5: Commit**

```bash
git add src/components/FileList.tsx src/components/FileList.css src/components/Summary.tsx src/__tests__/FileList.test.tsx
git commit -m "feat: FileList + Summary components"
```

---

## Task 9: SettingsPanel component

**Files:**
- Create: `src/components/SettingsPanel.tsx`
- Create: `src/components/SettingsPanel.css`
- Create: `src/__tests__/SettingsPanel.test.tsx`

- [ ] **Step 9.1: Write failing tests**

Create `src/__tests__/SettingsPanel.test.tsx`:

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SettingsPanel } from "../components/SettingsPanel";
import { DEFAULT_SETTINGS } from "../types";

describe("SettingsPanel", () => {
  it("is collapsed by default", () => {
    render(
      <SettingsPanel settings={DEFAULT_SETTINGS} onChange={vi.fn()} />
    );
    expect(screen.queryByLabelText(/quality/i)).not.toBeInTheDocument();
  });

  it("expands when gear icon is clicked", async () => {
    const user = userEvent.setup();
    render(
      <SettingsPanel settings={DEFAULT_SETTINGS} onChange={vi.fn()} />
    );
    await user.click(screen.getByRole("button", { name: /settings/i }));
    expect(screen.getByLabelText(/quality/i)).toBeInTheDocument();
  });

  it("shows format dropdown", async () => {
    const user = userEvent.setup();
    render(
      <SettingsPanel settings={DEFAULT_SETTINGS} onChange={vi.fn()} />
    );
    await user.click(screen.getByRole("button", { name: /settings/i }));
    expect(screen.getByLabelText(/format/i)).toBeInTheDocument();
  });

  it("calls onChange when lossless is toggled", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <SettingsPanel settings={DEFAULT_SETTINGS} onChange={onChange} />
    );
    await user.click(screen.getByRole("button", { name: /settings/i }));
    await user.click(screen.getByLabelText(/lossless/i));
    expect(onChange).toHaveBeenCalledWith({ lossless: true });
  });
});
```

Run: `npm test`
Expected: FAIL.

- [ ] **Step 9.2: Implement SettingsPanel**

Create `src/components/SettingsPanel.tsx`:

```tsx
import { useState } from "react";
import type { Settings } from "../types";
import { FORMAT_OPTIONS } from "../types";
import "./SettingsPanel.css";

interface SettingsPanelProps {
  settings: Settings;
  onChange: (update: Partial<Settings>) => void;
}

export function SettingsPanel({ settings, onChange }: SettingsPanelProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="settings-panel">
      <button
        className="settings-panel__toggle"
        onClick={() => setExpanded(!expanded)}
        aria-label="Settings"
        title="Settings"
      >
        {expanded ? "▾ Settings" : "⚙ Settings"}
      </button>

      {expanded && (
        <div className="settings-panel__body">
          <div className="settings-panel__field">
            <label htmlFor="quality">Quality</label>
            <div className="settings-panel__quality-row">
              <input
                id="quality"
                type="range"
                min="0"
                max="100"
                value={settings.quality ?? 0}
                disabled={settings.quality === null}
                onChange={(e) =>
                  onChange({ quality: parseInt(e.target.value, 10) })
                }
              />
              <label className="settings-panel__auto-label">
                <input
                  type="checkbox"
                  checked={settings.quality === null}
                  onChange={(e) =>
                    onChange({ quality: e.target.checked ? null : 80 })
                  }
                />
                Auto
              </label>
              {settings.quality !== null && (
                <span className="settings-panel__quality-value">
                  {settings.quality}
                </span>
              )}
            </div>
          </div>

          <div className="settings-panel__field">
            <label htmlFor="format">Format</label>
            <select
              id="format"
              value={settings.format ?? ""}
              onChange={(e) =>
                onChange({ format: e.target.value || null })
              }
            >
              {FORMAT_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
          </div>

          <div className="settings-panel__field">
            <label>
              <input
                type="checkbox"
                checked={settings.lossless}
                onChange={(e) => onChange({ lossless: e.target.checked })}
                aria-label="Lossless"
              />
              {" "}Lossless compression
            </label>
          </div>
        </div>
      )}
    </div>
  );
}
```

Create `src/components/SettingsPanel.css`:

```css
.settings-panel {
  margin: 0 16px;
}

.settings-panel__toggle {
  background: none;
  border: none;
  font-size: 13px;
  color: #888;
  cursor: pointer;
  padding: 4px 0;
}

.settings-panel__toggle:hover {
  color: #333;
}

.settings-panel__body {
  padding: 12px 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.settings-panel__field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.settings-panel__field label {
  font-size: 13px;
  color: #555;
}

.settings-panel__field select {
  padding: 6px 8px;
  border: 1px solid #ddd;
  border-radius: 4px;
  font-size: 13px;
}

.settings-panel__quality-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.settings-panel__quality-row input[type="range"] {
  flex: 1;
}

.settings-panel__auto-label {
  font-size: 12px;
  display: flex;
  align-items: center;
  gap: 4px;
}

.settings-panel__quality-value {
  font-size: 13px;
  font-weight: 600;
  min-width: 28px;
  text-align: right;
}
```

- [ ] **Step 9.3: Run tests**

```bash
npm test
```

Expected: all 4 SettingsPanel tests pass.

- [ ] **Step 9.4: Commit**

```bash
git add src/components/SettingsPanel.tsx src/components/SettingsPanel.css src/__tests__/SettingsPanel.test.tsx
git commit -m "feat: SettingsPanel component with quality/format/lossless controls"
```

---

## Task 10: useSquish hook

**Files:**
- Create: `src/hooks/useSquish.ts`
- Create: `src/__tests__/useSquish.test.tsx`

- [ ] **Step 10.1: Write failing tests**

Create `src/__tests__/useSquish.test.tsx`:

```tsx
import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useSquish } from "../hooks/useSquish";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Settings, BatchResult } from "../types";

const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);

describe("useSquish", () => {
  let dispatch: ReturnType<typeof vi.fn>;
  let settings: Settings;

  beforeEach(() => {
    vi.clearAllMocks();
    dispatch = vi.fn();
    settings = { quality: null, lossless: false, format: null };

    // Default: listen returns an unlisten function.
    mockListen.mockResolvedValue(() => {});
  });

  it("sets up event listeners on mount", () => {
    renderHook(() => useSquish(dispatch, settings));
    expect(mockListen).toHaveBeenCalledWith("squish://file-start", expect.any(Function));
    expect(mockListen).toHaveBeenCalledWith("squish://file-done", expect.any(Function));
    expect(mockListen).toHaveBeenCalledWith("squish://file-error", expect.any(Function));
  });

  it("squishFiles invokes Tauri command with correct args", async () => {
    const batchResult: BatchResult = {
      total_files: 1,
      success_count: 1,
      error_count: 0,
      skipped_count: 0,
      total_input_bytes: 100,
      total_output_bytes: 50,
      total_duration_ms: 200,
    };
    mockInvoke.mockResolvedValue(batchResult);

    const { result } = renderHook(() => useSquish(dispatch, settings));

    await act(async () => {
      await result.current.squishFiles(["/tmp/a.png"]);
    });

    expect(mockInvoke).toHaveBeenCalledWith("squish_files", {
      paths: ["/tmp/a.png"],
      options: { quality: null, lossless: false, format: null },
    });
  });
});
```

Run: `npm test`
Expected: FAIL.

- [ ] **Step 10.2: Implement useSquish**

Create `src/hooks/useSquish.ts`:

```typescript
import { useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  Settings,
  AppAction,
  FileStartPayload,
  FileDonePayload,
  FileErrorPayload,
  BatchResult,
} from "../types";

export function useSquish(
  dispatch: React.Dispatch<AppAction>,
  settings: Settings
) {
  const settingsRef = useRef(settings);
  settingsRef.current = settings;

  useEffect(() => {
    const unlisteners: Array<() => void> = [];

    async function setup() {
      const u1 = await listen<FileStartPayload>("squish://file-start", (event) => {
        dispatch({ type: "FILE_START", payload: event.payload });
      });
      unlisteners.push(u1);

      const u2 = await listen<FileDonePayload>("squish://file-done", (event) => {
        dispatch({ type: "FILE_DONE", payload: event.payload });
      });
      unlisteners.push(u2);

      const u3 = await listen<FileErrorPayload>("squish://file-error", (event) => {
        dispatch({
          type: "FILE_ERROR",
          id: event.payload.id,
          error: event.payload.error,
        });
      });
      unlisteners.push(u3);
    }

    setup();

    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, [dispatch]);

  const squishFiles = useCallback(
    async (paths: string[]): Promise<BatchResult | null> => {
      try {
        const result = await invoke<BatchResult>("squish_files", {
          paths,
          options: {
            quality: settingsRef.current.quality,
            lossless: settingsRef.current.lossless,
            format: settingsRef.current.format,
          },
        });
        dispatch({ type: "BATCH_COMPLETE" });
        return result;
      } catch (err) {
        console.error("squish_files failed:", err);
        dispatch({ type: "BATCH_COMPLETE" });
        return null;
      }
    },
    [dispatch]
  );

  return { squishFiles };
}
```

- [ ] **Step 10.3: Run tests**

```bash
npm test
```

Expected: all useSquish tests pass.

- [ ] **Step 10.4: Commit**

```bash
git add src/hooks/useSquish.ts src/__tests__/useSquish.test.tsx
git commit -m "feat: useSquish hook bridging Tauri IPC to React state"
```

---

## Task 11: App.tsx — wire everything together

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/App.css`
- Create: `src/__tests__/App.test.tsx`

- [ ] **Step 11.1: Write failing tests for App reducer**

Create `src/__tests__/App.test.tsx`:

```tsx
import { describe, it, expect } from "vitest";
import { appReducer, initialState, loadSettings } from "../App";
import type { AppState, FileDonePayload } from "../types";

describe("appReducer", () => {
  it("START_BATCH transitions to processing and clears files", () => {
    const state = appReducer(initialState(), { type: "START_BATCH" });
    expect(state.status).toBe("processing");
    expect(state.files).toHaveLength(0);
  });

  it("START_BATCH clears previous files", () => {
    const prev: AppState = {
      ...initialState(),
      status: "done",
      files: [
        { id: "old", filename: "old.png", path: "/old.png", status: "done" },
      ],
    };
    const state = appReducer(prev, { type: "START_BATCH" });
    expect(state.files).toHaveLength(0);
  });

  it("FILE_START adds a new file entry with compressing status", () => {
    const prev: AppState = {
      ...initialState(),
      status: "processing",
    };
    const state = appReducer(prev, {
      type: "FILE_START",
      payload: { id: "1", filename: "a.png", path: "/a.png" },
    });
    expect(state.files).toHaveLength(1);
    expect(state.files[0].status).toBe("compressing");
    expect(state.files[0].filename).toBe("a.png");
  });

  it("FILE_DONE updates file with result data", () => {
    const prev: AppState = {
      ...initialState(),
      status: "processing",
      files: [{ id: "1", filename: "a.png", path: "/a.png", status: "compressing" }],
    };
    const payload: FileDonePayload = {
      id: "1",
      input_bytes: 100_000,
      output_bytes: 30_000,
      output_path: "/a_squished.png",
      reduction_percent: 70.0,
      duration_ms: 500,
    };
    const state = appReducer(prev, { type: "FILE_DONE", payload });
    expect(state.files[0].status).toBe("done");
    expect(state.files[0].inputBytes).toBe(100_000);
    expect(state.files[0].reductionPercent).toBe(70.0);
  });

  it("FILE_ERROR marks file as errored", () => {
    const prev: AppState = {
      ...initialState(),
      status: "processing",
      files: [
        { id: "1", filename: "a.png", path: "/a.png", status: "compressing" },
      ],
    };
    const state = appReducer(prev, {
      type: "FILE_ERROR",
      id: "1",
      error: "decode failed",
    });
    expect(state.files[0].status).toBe("error");
    expect(state.files[0].error).toBe("decode failed");
  });

  it("BATCH_COMPLETE transitions to done", () => {
    const prev: AppState = {
      ...initialState(),
      status: "processing",
      files: [
        { id: "1", filename: "a.png", path: "/a.png", status: "done" },
      ],
    };
    const state = appReducer(prev, { type: "BATCH_COMPLETE" });
    expect(state.status).toBe("done");
  });

  it("UPDATE_SETTINGS merges partial settings", () => {
    const state = appReducer(initialState(), {
      type: "UPDATE_SETTINGS",
      settings: { lossless: true },
    });
    expect(state.settings.lossless).toBe(true);
    expect(state.settings.quality).toBeNull(); // unchanged
  });
});

describe("loadSettings", () => {
  it("returns defaults when localStorage is empty", () => {
    const s = loadSettings();
    expect(s.quality).toBeNull();
    expect(s.lossless).toBe(false);
    expect(s.format).toBeNull();
  });
});
```

Run: `npm test`
Expected: FAIL — `appReducer`, `initialState`, `loadSettings` not exported from `App`.

- [ ] **Step 11.2: Implement App.tsx**

Replace `src/App.tsx`:

```tsx
import { useReducer, useCallback, useState } from "react";
import { DropZone } from "./components/DropZone";
import { FileList } from "./components/FileList";
import { SettingsPanel } from "./components/SettingsPanel";
import { useSquish } from "./hooks/useSquish";
import type {
  AppState,
  AppAction,
  Settings,
  BatchResult,
} from "./types";
import "./App.css";

const SETTINGS_KEY = "squish-settings";

export function loadSettings(): Settings {
  try {
    const stored = localStorage.getItem(SETTINGS_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      return {
        quality: parsed.quality ?? null,
        lossless: parsed.lossless ?? false,
        format: parsed.format ?? null,
      };
    }
  } catch {
    // Corrupted localStorage — use defaults.
  }
  return { quality: null, lossless: false, format: null };
}

function saveSettings(settings: Settings) {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  } catch {
    // localStorage full or unavailable — silently ignore.
  }
}

export function initialState(): AppState {
  return {
    status: "idle",
    files: [],
    settings: loadSettings(),
  };
}

export function appReducer(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "START_BATCH":
      return {
        ...state,
        status: "processing",
        files: [],
      };

    case "FILE_START":
      return {
        ...state,
        files: [
          ...state.files,
          {
            id: action.payload.id,
            filename: action.payload.filename,
            path: action.payload.path,
            status: "compressing",
          },
        ],
      };

    case "FILE_DONE":
      return {
        ...state,
        files: state.files.map((f) =>
          f.id === action.payload.id
            ? {
                ...f,
                status: "done",
                inputBytes: action.payload.input_bytes,
                outputBytes: action.payload.output_bytes,
                outputPath: action.payload.output_path,
                reductionPercent: action.payload.reduction_percent,
                durationMs: action.payload.duration_ms,
              }
            : f
        ),
      };

    case "FILE_ERROR":
      return {
        ...state,
        files: state.files.map((f) =>
          f.id === action.id ? { ...f, status: "error", error: action.error } : f
        ),
      };

    case "BATCH_COMPLETE":
      return { ...state, status: "done" };

    case "UPDATE_SETTINGS": {
      const newSettings = { ...state.settings, ...action.settings };
      saveSettings(newSettings);
      return { ...state, settings: newSettings };
    }

    default:
      return state;
  }
}

function App() {
  const [state, dispatch] = useReducer(appReducer, undefined, initialState);
  const [batchResult, setBatchResult] = useState<BatchResult | null>(null);
  const { squishFiles } = useSquish(dispatch, state.settings);

  const handleDrop = useCallback(
    async (paths: string[]) => {
      if (state.status === "processing") return;

      setBatchResult(null);
      dispatch({ type: "START_BATCH" });

      // Rust side expands directories, emits file-start events (which add rows),
      // then processes files and emits file-done/file-error events.
      const result = await squishFiles(paths);
      if (result) {
        setBatchResult(result);
      }
    },
    [state.status, squishFiles]
  );

  const handleSettingsChange = useCallback((update: Partial<Settings>) => {
    dispatch({ type: "UPDATE_SETTINGS", settings: update });
  }, []);

  return (
    <div className="app">
      <DropZone status={state.status} onDrop={handleDrop} />
      <SettingsPanel settings={state.settings} onChange={handleSettingsChange} />
      <FileList files={state.files} batchResult={batchResult} />
    </div>
  );
}

export default App;
```

- [ ] **Step 11.3: Update App.css**

Replace `src/App.css`:

```css
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto,
    Helvetica, Arial, sans-serif;
  background: #f8f9fa;
  color: #333;
  -webkit-font-smoothing: antialiased;
}

.app {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
}
```

- [ ] **Step 11.4: Run tests**

```bash
npm test
```

Expected: all tests pass, including the new App reducer tests.

- [ ] **Step 11.5: Commit**

```bash
git add src/App.tsx src/App.css src/__tests__/App.test.tsx
git commit -m "feat: App component with useReducer state machine + settings persistence"
```

---

## Task 12: Verify full integration

**Files:**
- Modify: `src/main.tsx` (if needed to clean up scaffold imports)

- [ ] **Step 12.1: Clean up main.tsx**

Ensure `src/main.tsx` is clean:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
```

Remove any scaffold CSS imports from `main.tsx` (e.g., `import "./styles.css"`).

- [ ] **Step 12.2: Run full test suite**

```bash
npm test
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all frontend and Rust tests pass.

- [ ] **Step 12.3: Launch the dev server**

```bash
cargo tauri dev
```

Expected: window opens at 500x600 with title "squish", showing the drop zone, settings gear, and no file list.

- [ ] **Step 12.4: Manual testing**

Test each item from the spec's manual testing checklist:

1. Drop a PNG file → verify `_squished.png` appears alongside original, row shows savings.
2. Drop a folder → verify all recognized files in top level are compressed.
3. Drop files during processing → verify they are ignored.
4. Expand settings → change quality to 50 → drop a file → verify compression uses quality 50.
5. Change format to WebP → drop a PNG → verify output is `.webp`.
6. Toggle lossless → drop a file → verify lossless compression.
7. Close and reopen app → verify settings persisted.
8. Drop a corrupt file + a good file → verify good file succeeds, corrupt shows error.

- [ ] **Step 12.5: Delete smoke test**

Remove the trivial smoke test now that real tests exist:

```bash
rm src/__tests__/smoke.test.ts
```

- [ ] **Step 12.6: Final commit**

```bash
git add -A
git commit -m "chore: clean up scaffold + verify full integration"
```

---

## Task 13: Create GitHub repo and push

- [ ] **Step 13.1: Create remote repo**

```bash
cd /Users/michaelandrejoda/Sites/squish-desktop
gh repo create MikeDre/squish-desktop --private --source=. --push
```

(Use `--public` instead of `--private` if preferred.)

- [ ] **Step 13.2: Verify remote**

```bash
git remote -v
git log --oneline
```

Expected: all commits pushed to `origin/main`.

---

## Post-implementation: Manual verification checklist

Before considering this plan complete:

- [ ] `cargo tauri dev` opens the app without errors
- [ ] Drop a PNG → `_squished.png` appears, row shows filename + savings
- [ ] Drop a folder → all recognized files compressed
- [ ] Drop during processing → ignored
- [ ] Settings expand/collapse works
- [ ] Quality slider changes quality
- [ ] Format dropdown converts output format
- [ ] Lossless toggle works
- [ ] Settings persist across app restart
- [ ] Corrupt file shows error in row, doesn't abort batch
- [ ] Summary bar shows total files, sizes, savings %, time
- [ ] Results clear on next drop
- [ ] `npm test` passes all tests
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml` passes

---

## Coverage map (spec → tasks)

| Spec section | Tasks |
|---|---|
| Repo structure / scaffold | 1 |
| squish-core git dependency | 2 |
| Tauri commands (get_version, squish_files) | 2, 3 |
| Event emission (file-start, file-done, file-error) | 3 |
| TypeScript types | 4 |
| Test setup (Vitest + RTL + Tauri mocks) | 5 |
| DropZone component | 6 |
| FileRow component | 7 |
| FileList + Summary components | 8 |
| SettingsPanel component | 9 |
| useSquish hook (Tauri bridge) | 10 |
| App state machine (useReducer) | 11 |
| Settings persistence (localStorage) | 11 |
| Window configuration | 1 (Step 1.5) |
| Full integration + manual testing | 12 |
| GitHub repo + push | 13 |

No spec requirements without a mapped task.
