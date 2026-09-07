use super::*;
use std::path::PathBuf;

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("hakana-invalidation-{}", rand::random::<u64>()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn analyze(
        &self,
        source: &str,
        previous: Option<(AnalysisResult, SuccessfulScanData)>,
    ) -> (AnalysisResult, SuccessfulScanData) {
        self.analyze_edits(&[("input.hack", source)], previous)
    }

    fn analyze_edits(
        &self,
        edits: &[(&str, &str)],
        previous: Option<(AnalysisResult, SuccessfulScanData)>,
    ) -> (AnalysisResult, SuccessfulScanData) {
        let changes = edits
            .iter()
            .map(|(name, source)| {
                let path = self.0.join(name).to_string_lossy().into_owned();
                fs::write(&path, source).unwrap();
                (path, FileStatus::Modified(0, 0))
            })
            .collect();
        let mut config = Config::new(self.0.to_string_lossy().into_owned(), FxHashSet::default());
        config.ast_diff = true;
        config.collect_goto_definition_locations = true;
        let (previous_result, previous_scan) = match previous {
            Some((result, scan)) => (Some(result), scan),
            None => (None, SuccessfulScanData::default()),
        };
        scan_and_analyze(
            vec![],
            None,
            None,
            Arc::new(config),
            None,
            1,
            false,
            "",
            Arc::new(Interner::default()),
            Some(previous_scan),
            previous_result,
            Some(changes),
            || {},
        )
        .unwrap()
    }
}

#[test]
fn top_level_dependency_is_reanalyzed() {
    let project = Project::new();
    let caller = "function takes_int(int $x): void {}\ntakes_int(foo());\n";
    let before = project.analyze_edits(
        &[
            ("input.hack", caller),
            ("foo.hack", "function foo(): int { return 1; }"),
        ],
        None,
    );
    assert!(before.0.emitted_issues.values().all(Vec::is_empty));
    let after = project.analyze_edits(
        &[("foo.hack", "function foo(): string { return 'bad'; }")],
        Some(before),
    );
    assert!(
        after
            .0
            .emitted_issues
            .values()
            .flatten()
            .any(|issue| issue.kind == IssueKind::InvalidArgument),
        "issues: {:?}; references: {:?}",
        after.0.emitted_issues,
        after.0.symbol_references
    );
}

#[test]
fn deleted_dependency_clears_cached_definition_locations() {
    for caller in ["foo();\n", "function caller(): void { foo(); }\n"] {
        let project = Project::new();
        let source =
            format!("function stable(): void {{ bar(); }}\nfunction bar(): void {{}}\n{caller}");
        let before = project.analyze_edits(
            &[
                ("input.hack", &source),
                ("foo.hack", "function foo(): void {}"),
            ],
            None,
        );
        let after = project.analyze_edits(&[("foo.hack", "\n")], Some(before));
        assert!(
            after
                .0
                .emitted_issues
                .values()
                .flatten()
                .any(|issue| issue.kind == IssueKind::NonExistentFunction)
        );
        let locations = after
            .0
            .definition_locations
            .values()
            .flat_map(|locations| locations.values())
            .map(|symbol| after.1.interner.lookup(&symbol.0))
            .collect::<Vec<_>>();
        assert_eq!(
            locations,
            vec!["bar"],
            "the deleted target must be removed and the safe function's location preserved"
        );
    }
}

#[test]
fn deleted_class_is_removed_from_populated_return_types() {
    let project = Project::new();
    let before = project.analyze_edits(
        &[
            ("input.hack", "function foo(): A { return new A(); }"),
            ("a.hack", "final class A {}"),
        ],
        None,
    );
    let after = project.analyze_edits(&[("a.hack", "\n")], Some(before));
    let foo = after.1.interner.get("foo").unwrap();
    let return_type = after.1.codebase.functionlike_infos[&(foo, StrId::EMPTY)]
        .return_type
        .as_ref()
        .unwrap();
    assert!(matches!(
        &return_type.types[0],
        hakana_code_info::t_atomic::TAtomic::TReference { .. }
    ));
    assert!(
        after
            .0
            .definition_locations
            .values()
            .all(FxHashMap::is_empty)
    );
}

#[test]
fn watcher_event_without_previous_scan_falls_back_to_discovery() {
    let project = Project::new();
    fs::write(project.0.join("input.hack"), "function foo(): void {}").unwrap();
    let config = Arc::new(Config::new(
        project.0.to_string_lossy().into_owned(),
        FxHashSet::default(),
    ));
    let result = scanner::scan_files(
        &vec![config.root_dir.clone()],
        None,
        &config,
        1,
        false,
        "",
        &Arc::new(Interner::default()),
        None,
        Some(FxHashMap::default()),
        None,
        None,
    )
    .unwrap();
    assert!(
        result
            .interner
            .get(project.0.join("input.hack").to_str().unwrap())
            .is_some()
    );
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn top_level_edit_matches_fresh_analysis() {
    let project = Project::new();
    let before = project.analyze("function foo(int $x): void {}\nfoo(1);\n", None);
    let source = "function foo(int $x): void {}\nfoo('bad');\n";
    let incremental = project.analyze(source, Some(before));
    let fresh = project.analyze(source, None);
    let kinds = |result: &AnalysisResult| {
        result
            .emitted_issues
            .values()
            .flatten()
            .map(|issue| issue.kind.to_string())
            .collect::<Vec<_>>()
    };
    assert!(!kinds(&fresh.0).is_empty());
    assert_eq!(kinds(&incremental.0), kinds(&fresh.0));
}

#[test]
fn fixing_parse_error_to_empty_file_clears_diagnostics() {
    let project = Project::new();
    let before = project.analyze("function broken( {", None);
    assert!(
        before
            .0
            .emitted_issues
            .values()
            .any(|issues| !issues.is_empty())
    );
    let after = project.analyze("\n", Some(before));
    assert!(after.0.emitted_issues.values().all(Vec::is_empty));
}

#[test]
fn removing_top_level_call_clears_definition_locations_and_references() {
    let project = Project::new();
    let before = project.analyze("function foo(): void {}\nfoo();\n", None);
    assert!(
        before
            .0
            .definition_locations
            .values()
            .any(|locations| !locations.is_empty())
    );
    let after = project.analyze("function foo(): void {}\n", Some(before));
    assert!(
        after
            .0
            .definition_locations
            .values()
            .all(FxHashMap::is_empty)
    );
    let file = after
        .1
        .interner
        .get(project.0.join("input.hack").to_str().unwrap())
        .unwrap();
    assert!(
        !after
            .0
            .symbol_references
            .symbol_references_to_symbols
            .contains_key(&(file, hakana_str::StrId::EMPTY))
    );
}

#[test]
fn deleted_file_removes_previous_resolved_names() {
    let project = Project::new();
    let before = project.analyze("function foo(): void {}\nfoo();", None);
    let path = project.0.join("input.hack").to_string_lossy().into_owned();
    let file = FilePath(before.1.interner.get(&path).unwrap());
    assert!(before.1.resolved_names.contains_key(&file));
    fs::remove_file(&path).unwrap();
    let config = Arc::new(Config::new(
        project.0.to_string_lossy().into_owned(),
        FxHashSet::default(),
    ));
    let after = scanner::scan_files(
        &vec![config.root_dir.clone()],
        None,
        &config,
        1,
        false,
        "",
        &Arc::new(Interner::default()),
        Some(before.1),
        Some(FxHashMap::from_iter([(path, FileStatus::Deleted)])),
        None,
        None,
    )
    .unwrap();
    assert!(!after.resolved_names.contains_key(&file));
}
