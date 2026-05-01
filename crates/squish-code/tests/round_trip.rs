use squish_code::{squish_code, CodeFormat, CodeOptions};
use std::fs;
use tempfile::TempDir;

fn write(tmp: &TempDir, name: &str, contents: &str) -> std::path::PathBuf {
    let p = tmp.path().join(name);
    fs::write(&p, contents).unwrap();
    p
}

#[test]
fn js_default_minifies() {
    let tmp = TempDir::new().unwrap();
    let input = write(
        &tmp,
        "app.js",
        "console.log('hi ' + 'world ' + 42 + ' more text');\n",
    );

    let result = squish_code(&input, &CodeOptions::default()).unwrap();
    assert!(result.output_path.exists());
    assert!(result.output_bytes < result.input_bytes);
    assert_eq!(result.format, CodeFormat::Js);
    assert!(result.output_path.to_string_lossy().ends_with("app.min.js"));
}

#[test]
fn js_safe_mode_preserves_identifiers() {
    let tmp = TempDir::new().unwrap();
    let input = write(
        &tmp,
        "app.js",
        "function greetUser(name) { return 'hi ' + name; }\nconsole.log(greetUser('world'));",
    );

    let opts = CodeOptions {
        safe: true,
        ..Default::default()
    };
    let result = squish_code(&input, &opts).unwrap();
    let body = fs::read_to_string(&result.output_path).unwrap();
    assert!(body.contains("greetUser"), "safe mode must not mangle: {body}");
}

#[test]
fn js_source_map_emitted() {
    let tmp = TempDir::new().unwrap();
    let input = write(&tmp, "app.js", "console.log('hello world');");

    let opts = CodeOptions {
        source_map: true,
        ..Default::default()
    };
    let result = squish_code(&input, &opts).unwrap();
    let map = result.source_map_path.expect("map path");
    assert!(map.exists());
    let body = fs::read_to_string(&map).unwrap();
    assert!(body.contains("\"version\""));
}

#[test]
fn ts_input_produces_js_output() {
    let tmp = TempDir::new().unwrap();
    let input = write(
        &tmp,
        "app.ts",
        "interface Foo { x: number; }\nconsole.log('y');",
    );

    let result = squish_code(&input, &CodeOptions::default()).unwrap();
    assert!(result.output_path.to_string_lossy().ends_with("app.min.js"));
    let body = fs::read_to_string(&result.output_path).unwrap();
    assert!(!body.contains("interface"), "interfaces must be erased: {body}");
}

#[test]
fn css_default_minifies() {
    let tmp = TempDir::new().unwrap();
    let input = write(
        &tmp,
        "style.css",
        "/* comment */\n.button {\n  color: red;\n  padding: 10px;\n}\n",
    );

    let result = squish_code(&input, &CodeOptions::default()).unwrap();
    assert!(result.output_bytes < result.input_bytes);
    let body = fs::read_to_string(&result.output_path).unwrap();
    assert!(!body.contains("/*"));
    assert!(body.contains(".button"));
}

#[test]
fn css_source_map_emitted() {
    let tmp = TempDir::new().unwrap();
    let input = write(&tmp, "style.css", ".a { color: red; }");

    let opts = CodeOptions {
        source_map: true,
        ..Default::default()
    };
    let result = squish_code(&input, &opts).unwrap();
    assert!(result.source_map_path.expect("map path").exists());
}

#[test]
fn html_default_minifies() {
    let tmp = TempDir::new().unwrap();
    let input = write(
        &tmp,
        "page.html",
        "<!DOCTYPE html>\n<html>\n  <!-- header -->\n  <body>\n    <p>   hi   </p>\n  </body>\n</html>\n",
    );

    let result = squish_code(&input, &CodeOptions::default()).unwrap();
    assert!(result.output_bytes < result.input_bytes);
    let body = fs::read_to_string(&result.output_path).unwrap();
    assert!(!body.contains("<!-- header"));
}

#[test]
fn json_minified() {
    let tmp = TempDir::new().unwrap();
    let input = write(
        &tmp,
        "data.json",
        "{\n  \"name\": \"squish\",\n  \"version\": \"0.2.1\"\n}\n",
    );

    let result = squish_code(&input, &CodeOptions::default()).unwrap();
    let body = fs::read_to_string(&result.output_path).unwrap();
    assert_eq!(body, r#"{"name":"squish","version":"0.2.1"}"#);
}

#[test]
fn json_malformed_errors() {
    let tmp = TempDir::new().unwrap();
    let input = write(&tmp, "data.json", "{x:1}");

    let err = squish_code(&input, &CodeOptions::default()).unwrap_err();
    assert!(matches!(err, squish_code::CodeError::ParseFailed { .. }));
}

#[test]
fn already_minified_is_no_op_ish() {
    let tmp = TempDir::new().unwrap();
    let input = write(&tmp, "tiny.js", "console.log(1);");

    let result = squish_code(&input, &CodeOptions::default()).unwrap();
    // Output should not be much larger than input.
    assert!(result.output_bytes <= result.input_bytes + 4);
}

#[test]
fn force_overwrite_reuses_path() {
    let tmp = TempDir::new().unwrap();
    let input = write(&tmp, "app.js", "console.log(1);");

    let opts = CodeOptions {
        force_overwrite: true,
        ..Default::default()
    };
    let r1 = squish_code(&input, &opts).unwrap();
    let r2 = squish_code(&input, &opts).unwrap();
    assert_eq!(r1.output_path, r2.output_path);
}
