use clap::Args;
use hakana_analyzer::config;
use rustc_hash::FxHashSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

#[derive(Args)]
pub struct FindExecutableArgs {
    /// The root directory that Hakana runs in. Defaults to the current directory
    #[arg(long, value_name = "PATH")]
    pub root: Option<String>,

    /// File to save output to
    #[arg(long, value_name = "PATH")]
    pub output: String,
}

pub fn handle(
    args: &FindExecutableArgs,
    root_dir: &str,
    cwd: &String,
    threads: u8,
    show_progress: bool,
) {
    let output_file = args.output.clone();
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
