# squish-code

Code minification library for [squish](https://github.com/MikeDre/squish) — the local CLI that optimises images, video, audio, and code.

Minifies JavaScript, TypeScript, CSS, HTML, and JSON with pure-Rust tooling — no Node runtime required:

| Language | Engine |
|---|---|
| JS / TS / JSX | `oxc_minifier` (mangle + dead-code elimination; `safe` mode for whitespace-only) |
| CSS | `lightningcss` |
| HTML | `minify-html` |
| JSON | `serde_json` |

Outputs use the `.min` convention (`app.js` → `app.min.js`); TypeScript and JSX become `.js`. Source maps available for JS/TS/CSS.

```rust
use squish_code::{squish_code, CodeOptions};

let result = squish_code("dist/app.js".as_ref(), &CodeOptions::default())?;
println!("{} → {} bytes", result.input_bytes, result.output_bytes);
```

Most users want the CLI: `brew install mikedre/tap/squish` or `cargo install squish-media-cli`. See the [squish README](https://github.com/MikeDre/squish) for full documentation.

## Licence

MIT
