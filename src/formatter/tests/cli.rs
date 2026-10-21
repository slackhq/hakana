use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

fn run(root: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hakana-format"))
        .current_dir(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn stdin_and_parse_errors() {
    let root = tempfile::tempdir().unwrap();
    let result = run(root.path(), &[], "function f():void{foo(1,2);}");
    assert!(result.status.success());
    assert_eq!(result.stdout, b"function f(): void {\n  foo(1, 2);\n}\n");
    assert!(result.stderr.is_empty());

    let error = run(root.path(), &[], "function f(:void{");
    assert_eq!(error.status.code(), Some(2));
    assert!(error.stdout.is_empty());
    assert!(
        String::from_utf8(error.stderr)
            .unwrap()
            .contains("File failed to parse")
    );
}

#[test]
fn check_and_inplace_do_not_rewrite_unchanged_or_invalid_files() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("source.hack");
    fs::write(&file, "function f():void{foo(1,2);}").unwrap();
    let result = run(root.path(), &["--check-formatting", "source.hack"], "");
    assert!(result.status.success());
    assert_eq!(result.stdout, b"source.hack\n");

    assert!(
        run(root.path(), &["-i", "source.hack"], "")
            .status
            .success()
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "function f(): void {\n  foo(1, 2);\n}\n"
    );
    let modified = fs::metadata(&file).unwrap().modified().unwrap();
    assert!(
        run(root.path(), &["-i", "source.hack"], "")
            .status
            .success()
    );
    assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), modified);
    assert!(
        run(root.path(), &["--check-formatting", "source.hack"], "")
            .stdout
            .is_empty()
    );

    let invalid = "function f(:void{";
    fs::write(&file, invalid).unwrap();
    let result = run(root.path(), &["-i", "source.hack"], "");
    assert!(!result.stderr.is_empty());
    assert_eq!(fs::read_to_string(&file).unwrap(), invalid);
}

#[test]
fn diff_dry_run_and_write_preserve_unselected_functions() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("source.hack");
    let source = "function first():void {foo(1,2);}\n\nfunction second():void {bar(3,4);}\n";
    let diff = "--- a/source.hack\n+++ b/source.hack\n@@ -1 +1 @@\n-old line\n+new line\n";
    fs::write(&file, source).unwrap();
    let result = run(root.path(), &["--diff-dry-run", "--root", "."], diff);
    assert!(result.status.success(), "{:?}", result.stderr);
    assert_eq!(fs::read_to_string(&file).unwrap(), source);
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.starts_with("*** source.hack\n"));
    assert!(output.contains("foo(1, 2);"));
    assert!(output.ends_with("\n\nfunction second():void {bar(3,4);}\n"));
    let result = run(root.path(), &["--diff", "--root", "."], diff);
    assert!(result.status.success(), "{:?}", result.stderr);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        output.trim_start_matches("*** source.hack\n")
    );
}
