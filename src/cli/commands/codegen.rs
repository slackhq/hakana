use clap::{Command, arg};
use hakana_analyzer::config;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_code_info::analysis_result::{CheckPointEntry, CheckPointEntryLevel};
use hakana_str::Interner;
use rustc_hash::FxHashSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::exit;
use std::sync::Arc;

pub fn get_subcommand() -> Command<'static> {
    Command::new("codegen")
        .about("Generates codegen")
        .arg(
            arg!(--"root" <PATH>)
                .required(false)
                .help("The root directory that Hakana runs in. Defaults to the current directory"),
        )
        .arg(
            arg!(--"config" <PATH>)
                .required(false)
                .help("Hakana config path — defaults to ./hakana.json"),
        )
        .arg(
            arg!(--"name" <PATH>)
                .required(false)
                .help("The codegen you want to perform — if omitted, all codegen is generated"),
        )
        .arg(
            arg!(--"check")
                .required(false)
                .help("If passed, will just verify that codegen is accurate"),
        )
        .arg(
            arg!(--"overwrite")
                .required(false)
                .help("If passed, will overwrite any conflicting files"),
        )
        .arg(
            arg!(--"threads" <PATH>)
                .required(false)
                .help("How many threads to use"),
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
}

pub fn handle(
    sub_matches: &clap::ArgMatches,
    root_dir: &str,
    all_custom_issues: FxHashSet<String>,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    mut codegen_hooks: Vec<Box<dyn CustomHook>>,
    config_path: Option<&Path>,
    cwd: &String,
    threads: u8,
    show_progress: bool,
    header: &str,
) {
    let codegen_name = sub_matches.value_of("name");
    let check_codegen = sub_matches.is_present("check");
    let overwrite_codegen = sub_matches.is_present("overwrite");

    let output_file = sub_matches.value_of("output").map(|f| f.to_string());

    let mut config = config::Config::new(root_dir.to_string(), all_custom_issues);
    config.hooks = analysis_hooks.into_iter().map(Arc::from).collect();
    config.in_codegen = true;

    if let Some(codegen_name) = codegen_name {
        codegen_hooks.retain(|hook| {
            if let Some(candidate_name) = hook.get_codegen_name() {
                candidate_name == codegen_name
            } else {
                false
            }
        });

        if codegen_hooks.is_empty() {
            println!("Codegen {} not recognised", codegen_name);
            exit(1);
        }
    }

    config
        .hooks
        .extend(codegen_hooks.into_iter().map(Arc::from));

    let config_path = config_path.unwrap();

    let mut interner = Interner::default();

    if config_path.exists() {
        config
            .update_from_file(cwd, config_path, &mut interner)
            .ok();
    }
    config.allowed_issues = None;

    let config = Arc::new(config);

    let result = hakana_orchestrator::scan_and_analyze(
        Vec::new(),
        None,
        None,
        config.clone(),
        None,
        threads,
        show_progress,
        header,
        Arc::new(interner),
        None,
        None,
        None,
        || {},
    );

    if let Ok(result) = result {
        let mut errors = vec![];
        let mut updated_count = 0;
        let mut verified_count = 0;

        let mut already_written_files = FxHashSet::default();

        for (name, info) in result.0.codegen {
            if already_written_files.contains(&name) {
                errors.push((name.clone(), "File already written".to_string()));
                continue;
            }

            already_written_files.insert(name.clone());

            let path = Path::new(&name);
            if !path.exists() {
                if check_codegen {
                    errors.push((name.clone(), "Doesn't exist".to_string()));
                    continue;
                }
            } else {
                match &info {
                    Ok(info) => {
                        let existing_contents = fs::read_to_string(path).unwrap();
                        if existing_contents.trim() != info.trim() {
                            if check_codegen || !overwrite_codegen {
                                errors.push((name, "differs from codegen".to_string()));
                                continue;
                            }
                        } else {
                            verified_count += 1;
                            continue;
                        }
                    }
                    Err(err) => {
                        errors.push((name, err.clone()));
                        continue;
                    }
                }
            }

            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir).unwrap();
            }

            let mut output_path = fs::File::create(path).unwrap();
            match info {
                Ok(info) => {
                    write!(output_path, "{}", &info).unwrap();
                    tty_println!("Saved {}", name);
                    updated_count += 1;
                }
                Err(err) => {
                    errors.push((name, err));
                    continue;
                }
            }
        }

        if let Some(output_file) = output_file {
            write_codegen_output_files(output_file, cwd, &errors);
        }

        if !errors.is_empty() {
            println!(
                "\nCodegen verification failed.\n\nUse hakana codegen --overwrite to regenerate\n\n{}\n\n",
                errors
                    .into_iter()
                    .map(|(k, v)| format!("Error: {}\n - {}", k, v))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            exit(1);
        }

        if check_codegen {
            tty_println!("\n{} codegen files verified!", verified_count);
        } else {
            tty_println!("\n{} files generated", updated_count);
        }
    }
}

fn write_codegen_output_files(output_file: String, cwd: &String, errors: &Vec<(String, String)>) {
    let output_path = if output_file.starts_with('/') {
        output_file
    } else {
        format!("{}/{}", cwd, output_file)
    };
    let mut output_path = fs::File::create(Path::new(&output_path)).unwrap();

    let mut checkpoint_entries = vec![];

    for (file_path, issue) in errors {
        checkpoint_entries.push(CheckPointEntry {
            case: "Codegen error".to_string(),
            level: CheckPointEntryLevel::Failure,
            filename: file_path.clone(),
            line: 1,
            output: issue.to_string(),
        });
    }

    let json = serde_json::to_string_pretty(&checkpoint_entries).unwrap();
    write!(output_path, "{}", json).unwrap();
}
