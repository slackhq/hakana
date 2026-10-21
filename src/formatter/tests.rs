use super::*;
use crate::{format, format_at_offset, format_intervals, format_range, syntax};

fn assert_stable(source: &str, config: &Config) -> String {
    let formatted = format(source, config).unwrap();
    assert_eq!(format(&formatted, config).unwrap(), formatted);
    formatted
}

fn token_stream(source: &str, config: &Config) -> Vec<(String, String)> {
    let (root, errors) = syntax::parse(source, config.enable_xhp_class_modifier);
    assert!(errors.is_empty(), "{errors:?}");
    let mut tokens = vec![];
    root.all_tokens(&mut tokens);
    tokens
        .iter()
        .filter(|t| t.kind != "Comma")
        .map(|t| {
            (
                t.kind.clone(),
                if t.kind == "Hashbang" {
                    String::new()
                } else {
                    t.text.clone()
                },
            )
        })
        .collect()
}

#[test]
fn upstream_full_file_idempotence() {
    fn visit(dir: &Path, count: &mut usize) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, count);
            } else if path.extension().is_some_and(|x| x == "php") {
                let flags = fs::read_to_string(path.with_extension("flags")).unwrap_or_default();
                let o = options(flags.split_whitespace().map(str::to_owned)).unwrap();
                let config = formatting_config(&o, path.parent().unwrap());
                let source = fs::read_to_string(&path).unwrap();
                let formatted = format(&source, &config).unwrap_or_else(|e| {
                    panic!("{}: {e}", path.display());
                });
                assert_eq!(
                    format(&formatted, &config).unwrap(),
                    formatted,
                    "Not byte-for-byte idempotent: {}",
                    path.display()
                );
                assert_eq!(
                    token_stream(&source, &config),
                    token_stream(&formatted, &config),
                    "Token stream changed: {}",
                    path.display()
                );
                *count += 1;
            }
        }
    }
    let upstream =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third-party/hhvm/hphp/hack/test");
    let mut count = 0;
    visit(&upstream.join("hackfmt/tests"), &mut count);
    visit(&upstream.join("enum_class_label/format"), &mut count);
    assert!(count >= 188, "Upstream fixtures must be available");
}

#[test]
fn tagless_unicode_comments_and_tokens_are_preserved() {
    let source = "// café\nfunction demo(string $x):string{ /* α */ return $x.\"🙂\"; }\n";
    let config = Config::default();
    let formatted = assert_stable(source, &config);
    assert!(formatted.starts_with("// café\nfunction demo("));
    assert!(formatted.contains("/* α */"));
    assert!(!formatted.contains("<?hh"));
    assert_eq!(
        token_stream(source, &config),
        token_stream(&formatted, &config)
    );
}

#[test]
fn format_intervals_preserves_unselected_text() {
    let source = "function first():void {foo(1,2);}\n\nfunction second():void {bar(3,4);}\n";
    let formatted = format_intervals(source, &Config::default(), &[(1, 1)]).unwrap();
    assert!(formatted.contains("foo(1, 2);"));
    assert!(formatted.ends_with("\n\nfunction second():void {bar(3,4);}\n"));
    assert_eq!(
        token_stream(source, &Config::default()),
        token_stream(&formatted, &Config::default())
    );
}

#[test]
fn generated_source_and_empty_input_are_stable() {
    for text in [
        "",
        "// @generated\nfunction x():void {f(1,2);}",
        "// @generated\n",
    ] {
        assert_eq!(assert_stable(text, &Config::default()), text);
    }
}

#[test]
fn offset_is_a_byte_position_and_invalid_ranges_are_errors() {
    let source = "function f():void { $x = \"🙂\"; }\n";
    let config = Config::default();
    let offset = source.find(';').unwrap();
    let (range, replacement) = format_at_offset(source, &config, offset).unwrap();
    assert!(replacement.contains("\"🙂\";"));
    assert_eq!(range.1, offset + 1);
    assert!(format_at_offset(source, &config, offset - 1).is_ok());
    assert!(format_at_offset(source, &config, source.len()).is_err());
    assert!(format_range(source, &config, 10, 2).is_err());
    assert!(format_range(source, &config, 0, source.len() + 1).is_err());
    assert!(format_intervals(source, &config, &[(2, 1)]).is_err());
}

#[test]
fn invalid_source_is_reported_without_a_partial_result() {
    let error = format("function f(: void {", &Config::default()).unwrap_err();
    assert_eq!(error.exit_code, 2);
    assert!(
        error
            .message
            .ends_with("File failed to parse without errors")
    );
}

#[test]
fn incompatible_cli_modes_are_rejected() {
    for args in [
        vec!["-i"],
        vec!["--range", "1", "2", "-i", "f.hack"],
        vec!["--diff", "--at-char", "2"],
        vec!["--check", "--line-range", "1", "2", "f.hack"],
    ] {
        let options = options(args.into_iter().map(str::to_owned)).unwrap();
        assert_eq!(validate(&options).unwrap_err().exit_code, 3);
    }
}
