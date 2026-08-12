use crate::error::CodeError;
use crate::format::CodeFormat;
use crate::languages::MinifyOutput;
use crate::options::CodeOptions;
use oxc_allocator::Allocator;
use oxc_codegen::{Codegen, CodegenOptions, CommentOptions};
use oxc_minifier::{Minifier, MinifierOptions};
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_transformer::{JsxOptions, TransformOptions, Transformer, TypeScriptOptions};
use std::path::Path;

pub fn minify(
    input: &str,
    opts: &CodeOptions,
    path: &Path,
    format: CodeFormat,
) -> Result<MinifyOutput, CodeError> {
    let allocator = Allocator::default();

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let is_jsx = CodeFormat::is_jsx_extension(ext);
    let is_ts = matches!(format, CodeFormat::Ts);

    let source_type = SourceType::default()
        .with_typescript(is_ts)
        .with_jsx(is_jsx)
        .with_module(true);

    let parser_ret = Parser::new(&allocator, input, source_type).parse();
    if !parser_ret.diagnostics.is_empty() {
        let first_err = &parser_ret.diagnostics[0];
        return Err(CodeError::ParseFailed {
            path: path.to_path_buf(),
            // OxcDiagnostic carries span info but not a bare line number;
            // the Display impl formats the message without a line, so we pass None.
            line: None,
            reason: first_err.to_string(),
        });
    }

    let mut program = parser_ret.program;

    if is_ts {
        // TS-only transform: erase types, lower enum/namespace/import-equals.
        // We intentionally do NOT enable decorator or ES-target transforms —
        // squish-code's contract is "minify, don't transform" beyond what's needed
        // for the output to be valid JS.
        //
        // Notes on the non-obvious options:
        //   * JsxOptions::disable() — JsxOptions::default() has jsx_plugin:true which would
        //     silently compile JSX to React runtime calls; we explicitly disable that.
        //   * SemanticBuilder::new().with_enum_eval(true) — the transformer panics on `enum`
        //     statements unless the scoping was produced with enum constant-folding enabled.
        let transform_opts = TransformOptions {
            typescript: TypeScriptOptions::default(),
            jsx: JsxOptions::disable(),
            ..TransformOptions::default()
        };
        // build_with_scoping requires semantic scoping, so run the semantic
        // analyser first to obtain a Scoping object.  Enum constant-folding must
        // be enabled in the semantic pass so that the transformer can lower enums.
        let scoping = SemanticBuilder::new()
            .with_enum_eval(true)
            .build(&program)
            .semantic
            .into_scoping();
        let transformer_ret = Transformer::new(&allocator, path, &transform_opts)
            .build_with_scoping(scoping, &mut program);
        if !transformer_ret.diagnostics.is_empty() {
            let first_err = &transformer_ret.diagnostics[0];
            return Err(CodeError::ParseFailed {
                path: path.to_path_buf(),
                line: None,
                reason: first_err.to_string(),
            });
        }
    }

    // Run the minifier (compress + mangle) unless safe mode is requested.
    // In safe mode we still use codegen's `minify: true` for whitespace
    // removal, but skip identifier mangling and dead-code elimination so
    // that identifier names and "dead" code are preserved.
    let scoping = if !opts.safe {
        let ret = Minifier::new(MinifierOptions::default()).minify(&allocator, &mut program);
        ret.scoping
    } else {
        None
    };

    let source_map_path = opts.source_map.then(|| path.to_path_buf());

    let codegen_opts = CodegenOptions {
        minify: true,
        source_map_path,
        comments: CommentOptions::disabled(),
        ..CodegenOptions::default()
    };

    let codegen_ret = Codegen::new()
        .with_options(codegen_opts)
        .with_scoping(scoping)
        .build(&program);

    let source_map = codegen_ret.map.map(|sm| sm.to_json_string());

    Ok(MinifyOutput {
        code: codegen_ret.code,
        source_map,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn minifies_simple_js() {
        // Use a top-level expression statement so DCE cannot remove it.
        // An arrow-function constant that is never called would be eliminated
        // by the default CompressOptions (unused: Remove), which is correct
        // behaviour but would break the assertion.
        let input = "console.log('hi ' + 'world');\n";
        let path = PathBuf::from("greet.js");
        let out = minify(input, &CodeOptions::default(), &path, CodeFormat::Js).unwrap();
        assert!(out.code.len() < input.len());
        assert!(out.code.contains("console.log"));
    }

    #[test]
    fn safe_mode_preserves_identifiers() {
        let input = "function greetUser(name) { return 'hi ' + name; }";
        let path = PathBuf::from("greet.js");
        let opts = CodeOptions {
            safe: true,
            ..Default::default()
        };
        let out = minify(input, &opts, &path, CodeFormat::Js).unwrap();
        assert!(
            out.code.contains("greetUser"),
            "safe mode must not mangle identifier names"
        );
    }

    #[test]
    fn full_mode_can_mangle() {
        let input = "function veryLongNameThatNoOneWouldUse(argumentWithLongName) { return argumentWithLongName + 1; }";
        let path = PathBuf::from("x.js");
        let out = minify(input, &CodeOptions::default(), &path, CodeFormat::Js).unwrap();
        // Either the function name OR the parameter name should be mangled.
        assert!(
            !out.code.contains("veryLongNameThatNoOneWouldUse")
                || !out.code.contains("argumentWithLongName"),
            "full minifier should mangle at least one of the long names: got {}",
            out.code
        );
    }

    #[test]
    fn ts_types_erased() {
        let input = "interface Foo { bar: string; }\nconst x: number = 1;\nconsole.log(x);";
        let path = PathBuf::from("x.ts");
        let out = minify(input, &CodeOptions::default(), &path, CodeFormat::Ts).unwrap();
        assert!(!out.code.contains("interface"));
        assert!(out.code.contains("console.log"));
    }

    #[test]
    fn jsx_parses() {
        let input = "const el = <div className=\"x\">hi</div>;\n";
        let path = PathBuf::from("c.jsx");
        let _ = minify(input, &CodeOptions::default(), &path, CodeFormat::Js).unwrap();
    }

    #[test]
    fn rejects_syntax_error() {
        let input = "const x = ;";
        let path = PathBuf::from("bad.js");
        let err = minify(input, &CodeOptions::default(), &path, CodeFormat::Js).unwrap_err();
        assert!(matches!(err, CodeError::ParseFailed { .. }));
    }

    #[test]
    fn source_map_emitted_when_requested() {
        let input = "const x = 1;\nconst y = 2;\n";
        let path = PathBuf::from("x.js");
        let opts = CodeOptions {
            source_map: true,
            ..Default::default()
        };
        let out = minify(input, &opts, &path, CodeFormat::Js).unwrap();
        let sm = out.source_map.expect("source map should be emitted");
        assert!(sm.contains("\"version\""));
    }

    #[test]
    fn no_source_map_by_default() {
        let input = "const x = 1;";
        let path = PathBuf::from("x.js");
        let out = minify(input, &CodeOptions::default(), &path, CodeFormat::Js).unwrap();
        assert!(out.source_map.is_none());
    }

    /// Parse `code` as JavaScript (NOT TypeScript) and assert no parse errors.
    /// Used to prove the transformer's output is valid JS, not TS.
    fn assert_valid_js(code: &str) {
        let alloc = oxc_allocator::Allocator::default();
        let st = oxc_span::SourceType::default().with_module(true);
        let ret = oxc_parser::Parser::new(&alloc, code, st).parse();
        assert!(
            ret.diagnostics.is_empty(),
            "output is not valid JS: {:?}\noutput was:\n{}",
            ret.diagnostics,
            code
        );
    }

    #[test]
    fn ts_enum_compiles_to_runtime_js() {
        let input = "enum Color { Red, Green }\nconsole.log(Color.Red);";
        let path = PathBuf::from("colors.ts");
        let out = minify(input, &CodeOptions::default(), &path, CodeFormat::Ts).unwrap();
        assert!(
            !out.code.contains("enum "),
            "enum keyword leaked: {}",
            out.code
        );
        assert!(out.code.contains("console.log"));
        assert_valid_js(&out.code);
    }

    #[test]
    fn ts_namespace_compiles_to_iife() {
        let input = "namespace Util { export const X = 1; }\nconsole.log(Util.X);";
        let path = PathBuf::from("util.ts");
        let out = minify(input, &CodeOptions::default(), &path, CodeFormat::Ts).unwrap();
        assert!(
            !out.code.contains("namespace "),
            "namespace keyword leaked: {}",
            out.code
        );
        assert_valid_js(&out.code);
    }

    #[test]
    fn ts_import_equals_compiles() {
        let input = "import fs = require('fs');\nconsole.log(fs);";
        let path = PathBuf::from("legacy.ts");
        let out = minify(input, &CodeOptions::default(), &path, CodeFormat::Ts).unwrap();
        // The "import X =" syntax must not survive in the output.
        assert!(
            !out.code.contains("import ") || !out.code.contains(" = require"),
            "import-equals syntax leaked: {}",
            out.code
        );
        // require('fs') or an equivalent CJS-style reference should be present.
        assert!(
            out.code.contains("require") || out.code.contains("fs"),
            "expected CJS-style fs reference: {}",
            out.code
        );
        assert_valid_js(&out.code);
    }

    #[test]
    fn ts_const_enum_compiles_or_inlines() {
        let input = "const enum Direction { Up = 1, Down }\nconsole.log(Direction.Up);";
        let path = PathBuf::from("direction.ts");
        let out = minify(input, &CodeOptions::default(), &path, CodeFormat::Ts).unwrap();
        // Either the const enum is inlined to literals OR compiled to an IIFE.
        // Both are valid; what must not survive is the `enum` keyword itself.
        assert!(
            !out.code.contains("const enum "),
            "const enum keyword leaked: {}",
            out.code
        );
        assert!(
            !out.code.contains("enum "),
            "enum keyword leaked: {}",
            out.code
        );
        assert_valid_js(&out.code);
    }

    #[test]
    fn ts_decorator_passes_through_untouched() {
        // Decorators are Stage-3 JS and we intentionally do not transform them.
        // Re-parse the output in TS mode (where decorators are guaranteed accepted)
        // to confirm the decorator syntax round-trips.
        let input =
            "function frozen<T>(x: T) { return x; }\n@frozen class Foo {}\nconsole.log(Foo);";
        let path = PathBuf::from("dec.ts");
        let out = minify(input, &CodeOptions::default(), &path, CodeFormat::Ts).unwrap();
        let alloc = oxc_allocator::Allocator::default();
        let st = oxc_span::SourceType::default()
            .with_typescript(true)
            .with_module(true);
        let ret = oxc_parser::Parser::new(&alloc, &out.code, st).parse();
        assert!(
            ret.diagnostics.is_empty(),
            "output didn't re-parse: {:?}\noutput:\n{}",
            ret.diagnostics,
            out.code
        );
    }

    #[test]
    fn tsx_strips_types_keeps_jsx() {
        let input = "const el: JSX.Element = <div>hi</div>;\nconsole.log(el);";
        let path = PathBuf::from("c.tsx");
        let out = minify(input, &CodeOptions::default(), &path, CodeFormat::Ts).unwrap();
        // Types must be erased.
        assert!(
            !out.code.contains("JSX.Element"),
            "type annotation leaked: {}",
            out.code
        );
        // JSX must be preserved (we don't compile JSX).
        assert!(
            out.code.contains("<div") || out.code.contains("\"div\""),
            "JSX missing: {}",
            out.code
        );
    }

    #[test]
    fn ts_dts_produces_empty_or_trivial() {
        let input = "export interface Foo { x: number; }\nexport type Bar = string;";
        let path = PathBuf::from("app.d.ts");
        let out = minify(input, &CodeOptions::default(), &path, CodeFormat::Ts).unwrap();
        let trimmed = out.code.trim();
        // Acceptable outputs after type erasure: empty, or a minimal module shape preserved
        // by the transformer like "export{}". The codegen may or may not add a trailing
        // semicolon, so we accept both forms.
        assert!(
            trimmed.is_empty()
                || trimmed == "export{}"
                || trimmed == "export{};"
                || trimmed == "export {};"
                || trimmed == "export {}",
            "expected near-empty output for .d.ts, got: {trimmed:?}"
        );
    }
}
