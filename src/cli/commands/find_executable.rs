use clap::{Command, arg};
use hakana_analyzer::config;
use rustc_hash::FxHashSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

pub fn get_subcommand() -> Command<'static> {
    Command::new("find-executable")
        .about("Finds all executable lines of code")
        .arg(
            arg!(--"root" <PATH>)
                .required(false)
                .help("The root directory that Hakana runs in. Defaults to the current directory"),
        )
        .arg(
            arg!(--"output" <PATH>)
                .required(true)
                .help("File to save output to"),
        )
}

pub fn handle(
    sub_matches: &clap::ArgMatches,
    root_dir: &str,
    cwd: &String,
    threads: u8,
    show_progress: bool,
) {
    let output_file = sub_matches.value_of("output").unwrap().to_string();
    let config = config::Config::new(root_dir.to_string(), FxHashSet::default());

    match executable_finder::scan_files(
        &vec![root_dir.to_string()],
        None,
        &Arc::new(config),
        threads,
        show_progress,
    ) {
        Ok(file_infos) => {
            let output_path = if output_file.starts_with('/') {
                output_file
            } else {
                format!("{}/{}", cwd, output_file)
            };
            let mut out = fs::File::create(Path::new(&output_path)).unwrap();
            match write!(
                out,
                "{}",
                serde_json::to_string_pretty(&file_infos).unwrap()
            ) {
                Ok(_) => {
                    tty_println!("Done")
                }
                Err(err) => {
                    println!("error: {}", err)
                }
            }
        }
        Err(_) => {
            println!("error")
        }
    }
}
