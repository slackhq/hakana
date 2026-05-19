use clap::{Command, arg};
use hakana_code_info::analysis_result::{CheckPointEntry, CheckPointEntryLevel};
use line_break_map::LineBreakMap;
use rustc_hash::FxHashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkdir::WalkDir;

pub fn get_subcommand() -> Command<'static> {
    Command::new("lint")
        .about("Runs linters on Hack code (HHAST-compatible)")
        .arg(arg!(--"root" <PATH>).required(false).help(
            "The root directory that Hakana runs in. Defaults to the current directory",
        ))
        .arg(
            arg!(--"config" <PATH>)
                .required(false)
                .help("Path to hhast-lint.json config file — defaults to ./hhast-lint.json"),
        )
        .arg(
            arg!(--"threads" <COUNT>)
                .required(false)
                .help("How many threads to use"),
        )
        .arg(
            arg!(--"fix")
                .required(false)
                .help("Apply auto-fixes where available"),
        )
        .arg(
            arg!(--"add-fixmes")
                .required(false)
                .help("Add HHAST_FIXME comments for linter issues"),
        )
        .arg(
            arg!(--"linter" <NAME>)
                .required(false)
                .multiple(true)
                .help("Run specific linter(s) by HHAST name or kebab-case name. When used with --add-fixmes, only add fixmes for these linter(s)"),
        )
        .arg(
            arg!(--"no-codeowners")
                .required(false)
                .help("Skip files that have codeowners (listed in CODEOWNERS file)"),
        )
        .arg(
            arg!(--"debug-timings")
                .required(false)
                .help("Show timing information for each linter"),
        )
        .arg(
            arg!(--"debug")
                .required(false)
                .help("Add output for debugging"),
        )
        .arg(
            arg!(--"output" <PATH>)
                .required(false)
                .help("File to save output to"),
        )
        .arg(
            arg!([PATH] "Optional file or directory to lint (defaults to config roots)")
                .required(false)
                .multiple(true),
        )
}

pub fn handle(
    sub_matches: &clap::ArgMatches,
    root_dir: &str,
    had_error: &mut bool,
    custom_linters: Vec<Box<dyn hakana_lint::Linter>>,
) {
    use hakana_lint::{HhastLintConfig, examples};
    use rustc_hash::FxHashMap;
    use std::sync::Mutex;

    let config_path = sub_matches
        .value_of("config")
        .unwrap_or(&format!("{}/hhast-lint.json", root_dir))
        .to_string();

    let apply_fixes = sub_matches.is_present("fix");
    let add_fixmes = sub_matches.is_present("add-fixmes");
    let skip_codeowners = sub_matches.is_present("no-codeowners");
    let debug_timings = sub_matches.is_present("debug-timings");

    let specific_linters = Arc::new(
        sub_matches
            .values_of("linter")
            .map(|values| values.map(|s| s.to_string()).collect::<FxHashSet<_>>()),
    );

    let fixme_linters = Arc::new(if add_fixmes {
        if let Some(linters) = specific_linters.as_ref() {
            linters.iter().cloned().collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    });

    let (codeowner_patterns, codeowner_exact_files) = if skip_codeowners {
        parse_codeowners_files(root_dir)
    } else {
        (Vec::new(), FxHashSet::default())
    };

    let mut hhast_config = if Path::new(&config_path).exists() {
        match HhastLintConfig::from_file(Path::new(&config_path)) {
            Ok(config) => config,
            Err(e) => {
                println!("Error loading lint config: {}", e);
                *had_error = true;
                return;
            }
        }
    } else {
        tty_println!(
            "No hhast-lint.json found at {}. Using default configuration.",
            config_path
        );
        HhastLintConfig::default()
    };

    let mut registry = hakana_lint::linter::LinterRegistry::new();
    registry.register(Box::new(
        examples::no_empty_statements::NoEmptyStatementsLinter,
    ));
    registry.register(Box::new(
        examples::no_whitespace_at_end_of_line::NoWhitespaceAtEndOfLineLinter,
    ));
    registry.register(Box::new(
        examples::use_statement_without_kind::UseStatementWithoutKindLinter,
    ));
    registry.register(Box::new(
        examples::dont_discard_new_expressions::DontDiscardNewExpressionsLinter,
    ));
    registry.register(Box::new(
        examples::must_use_braces_for_control_flow::MustUseBracesForControlFlowLinter,
    ));
    registry.register(Box::new(examples::no_await_in_loop::NoAwaitInLoopLinter));
    registry.register(Box::new(
        examples::unused_use_clause::UnusedUseClauseLinter::new(),
    ));

    for linter in custom_linters {
        registry.register(linter);
    }

    if let Some(requested_linters) = specific_linters.as_ref() {
        for linter_name in requested_linters.iter() {
            let mut found = false;
            for linter in registry.all() {
                if let Some(hhast_name) = linter.hhast_name() {
                    let short_name = hhast_name.split('\\').last().unwrap_or(hhast_name);
                    if linter_name == hhast_name
                        || linter_name == short_name
                        || linter_name == linter.name()
                    {
                        if !hhast_config.extra_linters.contains(&hhast_name.to_string()) {
                            hhast_config.extra_linters.push(hhast_name.to_string());
                        }
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                eprintln!("Warning: Unknown linter '{}'", linter_name);
            }
        }
    }
    let hhast_config = Arc::new(hhast_config);

    let registry = Arc::new(registry);

    let paths_to_lint: Vec<String> = if let Some(paths) = sub_matches.values_of("PATH") {
        paths.map(|s| s.to_string()).collect()
    } else if !hhast_config.roots.is_empty() {
        hhast_config
            .roots
            .iter()
            .map(|r| format!("{}/{}", root_dir, r))
            .collect()
    } else {
        vec![root_dir.to_string()]
    };

    let mut files_to_lint = Vec::new();
    let mut skipped_codeowner_files = 0;
    for base_path in paths_to_lint {
        for entry in WalkDir::new(&base_path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let path_str = path.to_string_lossy();
            if !path_str.ends_with(".hack") && !path_str.ends_with(".php") {
                continue;
            }

            if skip_codeowners {
                let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
                let canonical_root = std::path::Path::new(&root_dir)
                    .canonicalize()
                    .unwrap_or_else(|_| std::path::PathBuf::from(&root_dir));

                let relative_path = if let Ok(rel) = canonical_path.strip_prefix(&canonical_root) {
                    format!("/{}", rel.to_string_lossy())
                } else {
                    format!("/{}", path_str.trim_start_matches("./"))
                };

                let has_codeowner = codeowner_exact_files.contains(&relative_path)
                    || codeowner_patterns.iter().any(|pattern| {
                        pattern.matches(&relative_path)
                            || pattern.matches(relative_path.trim_start_matches('/'))
                    });

                if has_codeowner {
                    skipped_codeowner_files += 1;
                    continue;
                }
            }

            files_to_lint.push(path.to_path_buf());
        }
    }

    if skip_codeowners && skipped_codeowner_files > 0 {
        tty_println!(
            "Skipped {} file(s) with codeowners",
            skipped_codeowner_files
        );
    }

    if files_to_lint.is_empty() {
        tty_println!("\nNo files to lint.");
        return;
    }

    let total_errors = Arc::new(Mutex::new(0usize));
    let total_files = Arc::new(Mutex::new(0usize));
    let total_fixed = Arc::new(Mutex::new(0usize));
    let lint_output = Arc::new(Mutex::new(Vec::new()));
    let checkpoint_entries = Arc::new(Mutex::new(Vec::new()));
    let linter_times = Arc::new(Mutex::new(rustc_hash::FxHashMap::<
        String,
        std::time::Duration,
    >::default()));

    let threads = if let Some(val) = sub_matches.value_of("threads").map(|f| f.to_string()) {
        val.parse::<usize>().unwrap_or(8)
    } else {
        8
    };

    let mut group_size = threads;

    if files_to_lint.len() < 4 * group_size {
        group_size = 1;
    }

    let mut path_groups: FxHashMap<usize, Vec<PathBuf>> = FxHashMap::default();

    for (i, path) in files_to_lint.into_iter().enumerate() {
        let group = i % group_size;
        path_groups.entry(group).or_insert_with(Vec::new).push(path);
    }

    let mut handles = vec![];

    let root_dir = root_dir.to_string();

    for (_, path_group) in path_groups {
        let hhast_config = hhast_config.clone();
        let registry = registry.clone();
        let total_errors = total_errors.clone();
        let total_files = total_files.clone();
        let total_fixed = total_fixed.clone();
        let lint_output = lint_output.clone();
        let checkpoint_entries = checkpoint_entries.clone();
        let linter_times = linter_times.clone();
        let root_dir = root_dir.clone();
        let fixme_linters = fixme_linters.clone();

        let handle = std::thread::spawn(move || {
            let lint_config = hakana_lint::LintConfig {
                allow_auto_fix: apply_fixes,
                apply_auto_fix: apply_fixes,
                add_fixmes,
                fixme_linters: (*fixme_linters).clone(),
                enabled_linters: Vec::new(),
                disabled_linters: Vec::new(),
                root_path: Some(PathBuf::from(&root_dir)),
            };

            for path in path_group {
                let path_str = path.to_string_lossy();

                let relative_path = if let Ok(rel) = path.strip_prefix(&root_dir) {
                    rel.to_string_lossy().to_string()
                } else {
                    path_str.to_string()
                };

                let mut file_linters: Vec<&dyn hakana_lint::Linter> = Vec::new();

                for linter in registry.all() {
                    if let Some(hhast_name) = linter.hhast_name() {
                        if hhast_config.is_linter_enabled(hhast_name, &relative_path) {
                            file_linters.push(linter.as_ref());
                        }
                    } else {
                        file_linters.push(linter.as_ref());
                    }
                }

                if file_linters.is_empty() {
                    continue;
                }

                let contents = match fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        lint_output
                            .lock()
                            .unwrap()
                            .push(format!("Error reading {}: {}", path_str, e));
                        continue;
                    }
                };

                *total_files.lock().unwrap() += 1;

                let line_break_map = LineBreakMap::new(contents.as_bytes());

                match hakana_lint::run_linters(&path, &contents, &file_linters, &lint_config) {
                    Ok(result) => {
                        let mut times = linter_times.lock().unwrap();
                        for (linter_name, duration) in &result.linter_times {
                            *times
                                .entry(linter_name.clone())
                                .or_insert(std::time::Duration::ZERO) += *duration;
                        }

                        if !result.errors.is_empty() {
                            *total_errors.lock().unwrap() += result.errors.len();
                            for error in &result.errors {
                                let (line, column) = hakana_lint::offset_to_line_column(
                                    &line_break_map,
                                    error.start_offset,
                                );
                                lint_output.lock().unwrap().push(format!(
                                    "{}:{}:{}: {} - {}",
                                    relative_path, line, column, error.severity, error.message
                                ));

                                checkpoint_entries.lock().unwrap().push(CheckPointEntry {
                                    case: error.linter_name.to_string(),
                                    level: CheckPointEntryLevel::Failure,
                                    filename: relative_path.clone(),
                                    line: line as u32,
                                    output: error.message.clone(),
                                });
                            }
                        }

                        if apply_fixes || add_fixmes {
                            let mut file_ops_applied = false;

                            if !result.file_operations.is_empty() {
                                for file_op in &result.file_operations {
                                    match &file_op.op_type {
                                        hakana_lint::FileOpType::Create => {
                                            if let Some(ref content) = file_op.content {
                                                let target_path = if file_op.path.is_absolute() {
                                                    file_op.path.clone()
                                                } else if let Some(parent) = path.parent() {
                                                    parent.join(&file_op.path)
                                                } else {
                                                    file_op.path.clone()
                                                };

                                                if let Some(parent_dir) = target_path.parent() {
                                                    if let Err(e) = fs::create_dir_all(parent_dir) {
                                                        lint_output.lock().unwrap().push(format!(
                                                            "Error creating directory {}: {}",
                                                            parent_dir.display(),
                                                            e
                                                        ));
                                                        continue;
                                                    }
                                                }

                                                match fs::write(&target_path, content) {
                                                    Ok(_) => {
                                                        lint_output.lock().unwrap().push(format!(
                                                            "Created: {}",
                                                            target_path.display()
                                                        ));
                                                        file_ops_applied = true;
                                                    }
                                                    Err(e) => {
                                                        lint_output.lock().unwrap().push(format!(
                                                            "Error creating {}: {}",
                                                            target_path.display(),
                                                            e
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                        hakana_lint::FileOpType::Delete => {
                                            let target_path = if file_op.path.is_absolute() {
                                                file_op.path.clone()
                                            } else if let Some(parent) = path.parent() {
                                                parent.join(&file_op.path)
                                            } else {
                                                file_op.path.clone()
                                            };

                                            match fs::remove_file(&target_path) {
                                                Ok(_) => {
                                                    lint_output.lock().unwrap().push(format!(
                                                        "Deleted: {}",
                                                        target_path.display()
                                                    ));
                                                    file_ops_applied = true;
                                                }
                                                Err(e) => {
                                                    lint_output.lock().unwrap().push(format!(
                                                        "Error deleting {}: {}",
                                                        target_path.display(),
                                                        e
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if let Some(fixed_source) = result.modified_source {
                                match fs::write(&path, fixed_source) {
                                    Ok(_) => {
                                        *total_fixed.lock().unwrap() += 1;
                                        if add_fixmes {
                                            lint_output
                                                .lock()
                                                .unwrap()
                                                .push(format!("Added fixmes: {}", relative_path));
                                        } else {
                                            lint_output
                                                .lock()
                                                .unwrap()
                                                .push(format!("Fixed: {}", relative_path));
                                        }
                                    }
                                    Err(e) => {
                                        lint_output.lock().unwrap().push(format!(
                                            "Error writing {} to {}: {}",
                                            if add_fixmes { "fixmes" } else { "fixes" },
                                            path_str,
                                            e
                                        ));
                                    }
                                }
                            } else if file_ops_applied {
                                *total_fixed.lock().unwrap() += 1;
                            }
                        }
                    }
                    Err(e) => {
                        lint_output
                            .lock()
                            .unwrap()
                            .push(format!("Error linting {}: {}", path_str, e));
                    }
                }
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let mut output = lint_output.lock().unwrap().clone();
    output.sort();
    for line in output.iter() {
        println!("{}", line);
    }

    let final_errors = *total_errors.lock().unwrap();
    let final_files = *total_files.lock().unwrap();
    let final_fixed = *total_fixed.lock().unwrap();

    if final_errors > 0 {
        *had_error = true;
    }

    if final_errors > 0 {
        tty_println!(
            "\nFound {} lint issue(s) in {} file(s)",
            final_errors,
            final_files
        );
        if (apply_fixes || add_fixmes) && final_fixed > 0 {
            if add_fixmes {
                tty_println!("Added fixmes to {} file(s)", final_fixed);
            } else {
                tty_println!("Fixed {} file(s)", final_fixed);
            }
        }
    } else {
        tty_println!("\nNo lint issues found! Checked {} file(s).", final_files);
    }

    if debug_timings {
        let times = linter_times.lock().unwrap();
        if !times.is_empty() {
            println!("\nLinter timing statistics:");
            let mut sorted_times: Vec<_> = times.iter().collect();
            sorted_times.sort_by(|a, b| b.1.cmp(a.1));

            let total_time: std::time::Duration = times.values().sum();

            for (linter_name, duration) in sorted_times {
                let secs = duration.as_secs_f64();
                let percentage = if total_time.as_secs_f64() > 0.0 {
                    (secs / total_time.as_secs_f64()) * 100.0
                } else {
                    0.0
                };
                println!(
                    "  {:<50} {:>8.3}s ({:>5.1}%)",
                    linter_name, secs, percentage
                );
            }
            println!("  {:<50} {:>8.3}s", "Total", total_time.as_secs_f64());
        }
    }

    if let Some(output_path) = sub_matches.value_of("output") {
        let entries = checkpoint_entries.lock().unwrap();
        let json = serde_json::to_string_pretty(&*entries).unwrap();
        let mut output_file = fs::File::create(Path::new(output_path)).unwrap();
        write!(output_file, "{}", json).unwrap();
    }
}

fn parse_codeowners_files(root_dir: &str) -> (Vec<glob::Pattern>, FxHashSet<String>) {
    let mut codeowner_patterns = Vec::new();
    let mut exact_files = FxHashSet::default();

    let codeowners_path = format!("{}/.github/CODEOWNERS", root_dir);
    let codeowners_content = match fs::read_to_string(&codeowners_path) {
        Ok(content) => content,
        Err(_) => {
            let alt_path = format!("{}/CODEOWNERS", root_dir);
            match fs::read_to_string(&alt_path) {
                Ok(content) => content,
                Err(_) => return (codeowner_patterns, exact_files),
            }
        }
    };

    for line in codeowners_content.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let pattern = parts[0];

        if parts.len() < 2 {
            continue;
        }

        if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
            let glob_pattern = if pattern.starts_with('/') {
                pattern.to_string()
            } else if pattern.starts_with("**/") {
                pattern.to_string()
            } else {
                format!("**/{}", pattern)
            };

            if let Ok(compiled) = glob::Pattern::new(&glob_pattern) {
                codeowner_patterns.push(compiled);
            }
        } else if pattern.ends_with('/') {
            let dir_pattern = if pattern.starts_with('/') {
                format!("{}**", pattern)
            } else {
                format!("**/{pattern}**")
            };

            if let Ok(compiled) = glob::Pattern::new(&dir_pattern) {
                codeowner_patterns.push(compiled);
            }
        } else {
            let is_file = pattern.ends_with(".hack")
                || pattern.ends_with(".php")
                || pattern.ends_with(".hhi")
                || pattern.contains('.');

            if is_file {
                if pattern.starts_with('/') {
                    exact_files.insert(pattern.to_string());
                } else {
                    exact_files.insert(format!("/{}", pattern));
                }
            } else {
                if pattern.starts_with('/') {
                    if let Ok(compiled) = glob::Pattern::new(&format!("{}/**", pattern)) {
                        codeowner_patterns.push(compiled);
                    }
                } else {
                    if let Ok(compiled) = glob::Pattern::new(&format!("**/{pattern}/**")) {
                        codeowner_patterns.push(compiled);
                    }
                }
            }
        }
    }

    (codeowner_patterns, exact_files)
}
