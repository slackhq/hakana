use super::*;

fn lint_options() -> DiscoveryOptions {
    DiscoveryOptions {
        sort_by_file_name: false,
        deduplicate: false,
        keep_explicit_files: false,
        ignore_errors: true,
    }
}

#[test]
fn filtered_discovery_skips_missing_paths_and_keeps_repeated_inputs() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("nested");
    fs::create_dir(&dir).unwrap();
    let first = root.path().join("first.hack");
    let second = dir.join("second.php");
    let stub = dir.join("stub.hhi");
    for path in [&first, &second, &stub] {
        fs::write(path, "").unwrap();
    }
    let paths = [
        first.clone(),
        root.path().join("missing.hack"),
        stub,
        dir,
        first.clone(),
    ];
    let files = discover_files(&paths, lint_options(), |path| {
        let path = path.to_string_lossy();
        path.ends_with(".hack") || path.ends_with(".php")
    })
    .unwrap();
    assert_eq!(files, vec![first.clone(), second, first]);

    let result = discover_files(
        &paths,
        DiscoveryOptions {
            ignore_errors: false,
            ..lint_options()
        },
        |_| true,
    );
    assert_eq!(
        result.unwrap_err().path(),
        Some(root.path().join("missing.hack").as_path())
    );
}

#[test]
#[cfg(unix)]
fn symlink_loops_can_be_reported_or_skipped_without_losing_other_files() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("src");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("a.hack"), "").unwrap();
    symlink(".", dir.join("loop")).unwrap();
    let linked = root.path().join("linked");
    symlink("src", &linked).unwrap();

    let files = discover_files(&[&linked], lint_options(), |_| true).unwrap();
    assert_eq!(files, vec![linked.join("a.hack")]);
    let result = discover_files(
        &[&linked],
        DiscoveryOptions {
            ignore_errors: false,
            ..lint_options()
        },
        |_| true,
    );
    assert!(result.unwrap_err().loop_ancestor().is_some());
}
