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

fn assert_same_output(a: &Output, b: &Output) {
    assert_eq!(a.status.code(), b.status.code());
    assert_eq!(a.stdout, b.stdout);
    assert_eq!(a.stderr, b.stderr);
}

#[test]
fn parallel_check_and_lint_preserve_order_skips_and_errors() {
    let root = tempfile::tempdir().unwrap();
    let mut paths = vec![];
    for i in 0..24 {
        let path = format!("f{i}.hack");
        fs::write(
            root.path().join(&path),
            format!("function f{i}():void{{foo(1,2);}}"),
        )
        .unwrap();
        paths.push(path);
    }
    fs::write(root.path().join("f3.hack"), "function broken(:").unwrap();
    fs::write(
        root.path().join("f5.hack"),
        "// @noformat\nfunction skipped():void{foo(1,2);}",
    )
    .unwrap();
    paths.insert(7, "ignored.txt".into());
    paths.insert(9, "f0.hack".into());
    fs::write(root.path().join("files.txt"), paths.join("\n")).unwrap();

    for mode in ["--check-formatting", "--linttool"] {
        let mut args = vec![mode, "--jobs", "1"];
        if mode == "--linttool" {
            args.push("@files.txt");
        } else {
            args.extend(paths.iter().map(String::as_str));
        }
        let sequential = run(root.path(), &args, "");
        args[2] = "4";
        let parallel = run(root.path(), &args, "");
        assert_same_output(&sequential, &parallel);
        assert!(parallel.status.success());
        assert!(!parallel.stdout.is_empty());
    }
    for jobs in ["0", "-1", "nope"] {
        assert_eq!(
            run(root.path(), &["--jobs", jobs, "f0.hack"], "")
                .status
                .code(),
            Some(3)
        );
    }
}

#[test]
fn parallel_inplace_stops_writes_at_the_first_read_error() {
    let root = tempfile::tempdir().unwrap();
    let original = "function f():void{foo(1,2);}";
    let mut paths = vec![];
    for i in 0..24 {
        let path = format!("f{i}.hack");
        fs::write(root.path().join(&path), original).unwrap();
        paths.push(path);
    }
    // Include duplicate names and distinct names for the same inode.
    fs::hard_link(root.path().join("f0.hack"), root.path().join("alias.hack")).unwrap();
    paths.insert(1, "alias.hack".into());
    paths.insert(2, "f0.hack".into());
    paths.insert(20, "missing.hack".into());
    let mut args = vec!["--in-place", "--jobs", "4"];
    args.extend(paths.iter().map(String::as_str));
    let parallel = run(root.path(), &args, "");
    assert_eq!(parallel.status.code(), Some(3));
    let snapshot: Vec<_> = (0..24)
        .map(|i| fs::read(root.path().join(format!("f{i}.hack"))).unwrap())
        .collect();
    assert_ne!(snapshot[0], original.as_bytes());
    assert_eq!(snapshot[23], original.as_bytes());

    for i in 0..24 {
        fs::write(root.path().join(format!("f{i}.hack")), original).unwrap();
    }
    args[2] = "1";
    let sequential = run(root.path(), &args, "");
    assert_same_output(&sequential, &parallel);
    for (i, expected) in snapshot.iter().enumerate() {
        assert_eq!(
            &fs::read(root.path().join(format!("f{i}.hack"))).unwrap(),
            expected
        );
    }
}

#[test]
fn parallel_diff_matches_sequential_and_does_not_write_on_missing_file() {
    let root = tempfile::tempdir().unwrap();
    let original = "function f():void{foo(1,2);}\n";
    let mut diff = String::new();
    for i in 0..12 {
        fs::write(root.path().join(format!("f{i}.hack")), original).unwrap();
        diff += &format!("--- a/f{i}.hack\n+++ b/f{i}.hack\n@@ -1 +1 @@\n-old\n+new\n");
    }
    fs::write(root.path().join("f2.hack"), "function broken(:").unwrap();
    let sequential = run(root.path(), &["--diff-dry-run", "--jobs", "1"], &diff);
    let parallel = run(root.path(), &["--diff-dry-run", "--jobs", "4"], &diff);
    assert_same_output(&sequential, &parallel);
    assert!(parallel.status.success());
    assert!(!parallel.stderr.is_empty());

    fs::remove_file(root.path().join("f11.hack")).unwrap();
    let sequential = run(root.path(), &["--diff", "--jobs", "1"], &diff);
    let parallel = run(root.path(), &["--diff", "--jobs", "4"], &diff);
    assert_same_output(&sequential, &parallel);
    assert_eq!(parallel.status.code(), Some(4));
    assert_eq!(
        fs::read_to_string(root.path().join("f0.hack")).unwrap(),
        original
    );
}
