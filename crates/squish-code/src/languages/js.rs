use crate::error::CodeError;
use crate::format::CodeFormat;
use crate::languages::MinifyOutput;
use crate::options::CodeOptions;
use oxc_allocator::Allocator;
use oxc_ast::ast::Statement;
use oxc_codegen::{Codegen, CodegenOptions, CommentOptions};
use oxc_minifier::{Minifier, MinifierOptions};
use oxc_parser::Parser;
use oxc_span::SourceType;
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
    if !parser_ret.errors.is_empty() {
        let first_err = &parser_ret.errors[0];
        return Err(CodeError::ParseFailed {
            path: path.to_path_buf(),
            // OxcDiagnostic carries span info but not a bare line number;
            // the Display impl formats the message without a line, so we pass None.
            line: None,
            reason: first_err.to_string(),
        });
    }

    let mut program = parser_ret.program;

    // For TypeScript input, strip purely type-level declarations from the
    // program body before running the minifier and codegen.  oxc 0.128 does
    // not include a transformer that erases TS syntax, so we handle it here
    // by removing statement variants whose entire purpose is type information
    // (interfaces and type aliases).  Runtime-affecting TS constructs (enums,
    // namespaces, import-equals) are left in place so the minifier can see
    // and process them.
    if is_ts {
        program.body.retain(|stmt| {
            !matches!(
                stmt,
                Statement::TSTypeAliasDeclaration(_) | Statement::TSInterfaceDeclaration(_)
            )
        });
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
        assert!(out.code.contains("greetUser"), "safe mode must not mangle identifier names");
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
}
