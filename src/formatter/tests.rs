use super::*;
use crate::{format, format_at_offset, format_intervals, format_range, syntax};
use parser_core_types::token_kind::TokenKind;

fn assert_stable(source: &str, config: &Config) -> String {
    let formatted = format(source, config).unwrap();
    assert_eq!(format(&formatted, config).unwrap(), formatted);
    formatted
}

fn token_stream(source: &str, config: &Config) -> Vec<(TokenKind, String)> {
    let (root, errors) = syntax::parse(source, config.enable_xhp_class_modifier);
    assert!(errors.is_empty(), "{errors:?}");
    let mut tokens = vec![];
    root.all_tokens(&mut tokens);
    tokens
        .iter()
        .filter(|t| t.kind != TokenKind::Comma)
        .map(|t| {
            (
                t.kind,
                if t.kind == TokenKind::Hashbang {
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
fn leading_blank_lines_are_removed_before_code_and_comments() {
    let config = Config::default();
    for body in [
        "function f(): void {}\n",
        "namespace Example;\n\nfunction f(): void {}\n",
        "use type Example\\Item;\n",
        "type Item = int;\n",
        "final class Item {}\n",
        "/** Header comment. */\nfunction f(): void {}\n",
        "// Header comment.\nfunction f(): void {}\n",
        "<<__EntryPoint>>\nfunction main(): void {}\n",
    ] {
        let expected = assert_stable(body, &config);
        assert!(!expected.starts_with('\n'));
        for prefix in ["", "\n", "\n\n\n", "\t \n", "\r\n", " \t\r\n\r\n", "   "] {
            let source = format!("{prefix}{body}");
            assert_eq!(
                assert_stable(&source, &config),
                expected,
                "prefix {prefix:?}, body {body:?}"
            );
        }
    }
    for source in ["\n", "\n\n\n", "\t \n", "\r\n\r\n"] {
        assert_eq!(assert_stable(source, &config), "");
    }
}

#[test]
fn blank_lines_after_headers_and_between_declarations_are_preserved() {
    let config = Config::default();
    for (source, expected) in [
        (
            "<?hh\n\n\n\nfunction f(): void {}\n",
            "<?hh\n\nfunction f(): void {}\n",
        ),
        (
            "#!/usr/bin/env hhvm\n\n\nfunction f(): void {}\n",
            "#!/usr/bin/env hhvm\n\nfunction f(): void {}\n",
        ),
        (
            "// c\n\nfunction f(): void {}\n",
            "// c\n\nfunction f(): void {}\n",
        ),
        (
            "\n\n// c\n\nfunction f(): void {}\n",
            "// c\n\nfunction f(): void {}\n",
        ),
        (
            "function a(): void {}\n\nfunction b(): void {}\n\nfunction c(): void {}\n",
            "function a(): void {}\n\nfunction b(): void {}\n\nfunction c(): void {}\n",
        ),
    ] {
        assert_eq!(assert_stable(source, &config), expected);
    }
}

#[test]
fn leading_blank_lines_are_removed_only_in_selected_ranges() {
    let config = Config::default();
    let first = "\n\nfunction first():void {foo(1,2);}\n";
    let second = "\nfunction second():void {bar(3,4);}\n";
    let source = format!("{first}{second}");
    let formatted_first = "function first(): void {\n  foo(1, 2);\n}\n";
    assert_eq!(
        format_range(&source, &config, 0, first.len()).unwrap(),
        formatted_first
    );
    assert_eq!(format_range(&source, &config, 0, 2).unwrap(), "");
    assert_eq!(
        format_intervals(&source, &config, &[(1, 3)]).unwrap(),
        format!("{formatted_first}{second}")
    );
    assert_eq!(
        format_intervals(&source, &config, &[(5, 5)]).unwrap(),
        format!("{first}\nfunction second(): void {{\n  bar(3, 4);\n}}\n")
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
        "\n\n// @generated\nfunction x():void {f(1,2);}",
    ] {
        assert_eq!(assert_stable(text, &Config::default()), text);
    }
}

#[test]
fn hakana_suppressions_are_positioned_like_hh_fixme() {
    let config = Config::default();
    let template = "function f(): void {\n  $a   =  1;\n  X\n  $y = bar(  1,2);\n  $b   =  1;\n  $x = foo(  1,2); X\n  $c   =  1;\n  $c   =  1;\n  X $q   =   $y->foo(  );\n  $d   =  1;\n  $d   =  1;\n  return  X  bar(1,\n    2);\n}\n";
    let hh_fixme = "/* HH_FIXME[4110] */";
    let expected = assert_stable(&template.replace('X', hh_fixme), &config);
    for comment in [
        "/* HAKANA_FIXME[MixedMethodCall] */",
        "/* HAKANA_IGNORE[MixedMethodCall] */",
        "/** HAKANA_FIXME[MixedMethodCall] */",
        "/* HHAST_FIXME[UnusedParameter] */",
    ] {
        assert_eq!(
            assert_stable(&template.replace('X', comment), &config),
            expected.replace(hh_fixme, comment),
            "comment {comment:?}"
        );
    }
    assert_eq!(
        assert_stable(&template.replace('X', "/* HAKANA_NOTE */"), &config),
        "function f(): void {\n  $a = 1;\n  /* HAKANA_NOTE */\n  $y = bar(1, 2);\n  $b = 1;\n  $x = foo(1, 2); /* HAKANA_NOTE */\n  $c = 1;\n  $c = 1;\n  /* HAKANA_NOTE */ $q = $y->foo();\n  $d = 1;\n  $d = 1;\n  return /* HAKANA_NOTE */ bar(1, 2);\n}\n"
    );
}

#[test]
fn lint_ignore_is_recognized_in_line_and_block_comments() {
    let config = Config::default();
    assert_eq!(
        assert_stable(
            "function f(): void {\n  // @lint-ignore\n  $a   =  bar(  );\n  $b   =  1;\n  $b   =  1;\n  /* @lint-ignore */\n  $c   =  bar(  );\n}\n",
            &config
        ),
        "function f(): void {\n  // @lint-ignore\n  $a   =  bar(  );\n  $b = 1;\n  $b = 1;\n  /* @lint-ignore */\n  $c   =  bar(  );\n}\n"
    );
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

#[test]
#[cfg(not(target_family = "wasm"))]
fn file_workers_overlap_but_results_are_consumed_in_order() {
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;

    let started = (Mutex::new(0), Condvar::new());
    let consumer_thread = std::thread::current().id();
    let mut results = vec![];
    process_files(
        &(0..12).collect::<Vec<_>>(),
        2,
        |i| {
            assert_ne!(std::thread::current().id(), consumer_thread);
            let (lock, ready) = &started;
            let mut count = lock.lock().unwrap();
            *count += 1;
            ready.notify_all();
            let (_count, timeout) = ready
                .wait_timeout_while(count, Duration::from_secs(5), |n| *n < 2)
                .unwrap();
            assert!(!timeout.timed_out(), "File work did not run concurrently");
            i * 2
        },
        |i, result| {
            assert_eq!(std::thread::current().id(), consumer_thread);
            results.push((*i, result));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(results, (0..12).map(|i| (i, i * 2)).collect::<Vec<_>>());
}
