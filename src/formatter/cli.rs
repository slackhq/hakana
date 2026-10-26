//! Command-line compatibility with hackfmt, shared by `hakana format` and the
//! small formatter development binary. Formatting never initializes analysis.
use crate::{Config, FormatError, ranges};
use clap::{Arg, ArgMatches, Command, value_parser};
use hakana_file_discovery::{DiscoveryOptions, discover_files};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};
fn error(message: impl Into<String>, exit_code: i32) -> FormatError {
    FormatError {
        message: message.into(),
        exit_code,
    }
}
fn invalid(message: impl Into<String>) -> FormatError {
    error(format!("Invalid argument: {}", message.into()), 3)
}
fn read(path: &Path) -> Result<String, FormatError> {
    fs::read_to_string(path).map_err(|e| invalid(format!("{}: {e}", path.display())))
}
fn stdin() -> Result<String, FormatError> {
    let mut s = String::new();
    io::stdin()
        .read_to_string(&mut s)
        .map_err(|e| invalid(e.to_string()))?;
    Ok(s)
}
fn config(root: &Path) -> Config {
    let mut c = Config::default();
    if let Ok(text) = fs::read_to_string(root.join(".hhconfig")) {
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=') {
                let value = value.trim();
                match key.trim() {
                    "hackfmt.indent_width" => {
                        if let Ok(n) = value.parse() {
                            c.indent_width = n
                        }
                    }
                    "hackfmt.line_width" => {
                        if let Ok(n) = value.parse() {
                            c.line_width = n
                        }
                    }
                    "hackfmt.tabs" => c.tabs = value == "true",
                    "hackfmt.add_trailing_commas" => c.add_trailing_commas = value != "false",
                    "hackfmt.format_generated_code" => c.format_generated_code = value == "true",
                    "hackfmt.version" => c.formatting_version = value.parse().ok(),
                    "enable_xhp_class_modifier" => c.enable_xhp_class_modifier = value == "true",
                    _ => (),
                }
            }
        }
    }
    c
}
fn root_for(file: Option<&str>) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let start = file
        .map(|f| {
            let p = cwd.join(f);
            if p.is_dir() {
                p
            } else {
                p.parent().unwrap_or(&cwd).to_path_buf()
            }
        })
        .unwrap_or_else(|| cwd.clone());
    for p in start.ancestors() {
        if p.join(".hhconfig").is_file() {
            return p.into();
        }
    }
    cwd
}
fn is_hack(path: impl AsRef<Path>) -> bool {
    matches!(
        path.as_ref().extension().and_then(|s| s.to_str()),
        Some("php" | "hack" | "hhi" | "hh")
    )
}

fn expand_paths(paths: &[String]) -> Result<Vec<String>, FormatError> {
    discover_files(
        paths,
        DiscoveryOptions {
            sort_by_file_name: true,
            deduplicate: true,
            keep_explicit_files: true,
            ignore_errors: false,
        },
        |path| is_hack(path),
    )
    .map_err(|e| invalid(e.to_string()))?
    .into_iter()
    .map(|path| {
        path.to_str()
            .map(str::to_owned)
            .ok_or_else(|| invalid(format!("Non-UTF-8 path: {}", path.display())))
    })
    .collect()
}

#[derive(Default)]
struct Options {
    files: Vec<String>,
    root: Option<PathBuf>,
    diff_root: Option<PathBuf>,
    range: Option<(usize, usize)>,
    line_range: Option<(usize, usize)>,
    at: Option<usize>,
    raw: bool,
    ide: bool,
    inplace: bool,
    check: bool,
    diff: bool,
    dry: bool,
    lint: bool,
    severity: Option<String>,
    logging_filename: Option<String>,
    indent: Option<usize>,
    width: Option<usize>,
    version: Option<u32>,
    tabs: bool,
    generated: bool,
    jobs: Option<usize>,
}
fn positive(s: &str) -> Result<usize, String> {
    match s.parse::<usize>() {
        Ok(0) => Err("must be at least 1".into()),
        Ok(n) => Ok(n),
        Err(e) => Err(e.to_string()),
    }
}
/// The `format` subcommand, shared by `hakana format` and `hakana-format`.
pub fn command() -> Command<'static> {
    let value = |name: &'static str, value_name: &'static str| {
        Arg::new(name)
            .long(name)
            .takes_value(true)
            .value_name(value_name)
    };
    let range = |name: &'static str| {
        Arg::new(name)
            .long(name)
            .takes_value(true)
            .number_of_values(2)
            .value_names(&["START", "END"])
            .value_parser(value_parser!(usize))
    };
    let flag = |name: &'static str| Arg::new(name).long(name);
    Command::new("format")
        .about("Format Hack source using hackfmt-compatible rules")
        .after_help(
            "Reads stdin when PATH is omitted.\nDirectories are searched recursively in in-place, check, and linttool modes.\nFollows symlinks and selects .hack, .php, .hhi, and .hh files.",
        )
        .args_override_self(true)
        .arg(
            Arg::new("paths")
                .value_name("PATH")
                .multiple_values(true)
                .help("Files or directories to format; @FILE lists paths in --linttool mode"),
        )
        .arg(flag("in-place").short('i').help("Update files in place"))
        .arg(
            flag("check-formatting")
                .alias("check")
                .help("Print files that need formatting"),
        )
        .arg(
            value("jobs", "N")
                .short('j')
                .value_parser(positive)
                .help("Maximum workers (default: CPUs, capped at 8; 1 runs sequentially)"),
        )
        .arg(range("range").help("Format a 1-based byte range (end exclusive)"))
        .arg(range("line-range").help("Format an inclusive line range"))
        .arg(range("test-raw-range").hide(true))
        .arg(
            value("at-char", "OFFSET")
                .value_parser(value_parser!(usize))
                .help("Format at a 0-based byte offset"),
        )
        .arg(
            value("indent-width", "N")
                .value_parser(value_parser!(usize))
                .help("Spaces per indentation level (default 2)"),
        )
        .arg(
            value("line-width", "N")
                .value_parser(value_parser!(usize))
                .help("Maximum line width (default 80)"),
        )
        .arg(flag("tabs").help("Indent with tabs"))
        .arg(
            value("version", "N")
                .value_parser(value_parser!(u32))
                .help("Select hackfmt formatting rules"),
        )
        .arg(flag("format-generated-code").help("Format generated sections"))
        .arg(value("root", "DIRECTORY").help("Directory containing .hhconfig"))
        .arg(
            value("diff-root", "DIRECTORY")
                .help("Directory that paths in --diff input are relative to"),
        )
        .arg(flag("diff").help("Format changed lines from a diff on stdin"))
        .arg(flag("diff-dry-run").help("Print --diff results instead of writing files"))
        .arg(flag("linttool").help("Report unformatted files as lint JSON"))
        .arg(value("linttool-severity", "SEVERITY").help("Severity for --linttool output"))
        .arg(value("filename-for-logging", "PATH").help("Filename to assume for stdin"))
        .arg(flag("test-ide").hide(true))
        .arg(flag("test").hide(true))
}
fn options_from(m: &ArgMatches) -> Options {
    let string = |name| m.get_one::<String>(name).cloned();
    let range = |name| {
        m.get_many::<usize>(name).map(|mut v| {
            let a = *v.next().unwrap();
            (a, *v.next().unwrap())
        })
    };
    let raw_range = range("test-raw-range");
    Options {
        files: m
            .get_many::<String>("paths")
            .into_iter()
            .flatten()
            .filter(|path| *path != "-")
            .cloned()
            .collect(),
        root: string("root").map(PathBuf::from),
        diff_root: string("diff-root").map(PathBuf::from),
        raw: raw_range.is_some(),
        range: raw_range.or_else(|| range("range")),
        line_range: range("line-range"),
        at: m.get_one("at-char").copied(),
        ide: m.is_present("test-ide"),
        inplace: m.is_present("in-place"),
        check: m.is_present("check-formatting"),
        diff: m.is_present("diff") || m.is_present("diff-dry-run"),
        dry: m.is_present("diff-dry-run"),
        lint: m.is_present("linttool"),
        severity: string("linttool-severity"),
        logging_filename: string("filename-for-logging"),
        indent: m.get_one("indent-width").copied(),
        width: m.get_one("line-width").copied(),
        version: m.get_one("version").copied(),
        tabs: m.is_present("tabs"),
        generated: m.is_present("format-generated-code"),
        jobs: m.get_one("jobs").copied(),
    }
}
fn options(args: impl Iterator<Item = String>) -> Result<Options, clap::Error> {
    let matches =
        command().try_get_matches_from(std::iter::once("hakana format".to_owned()).chain(args))?;
    Ok(options_from(&matches))
}
fn validate(o: &Options) -> Result<(), FormatError> {
    if o.lint {
        return Ok(());
    }
    if o.severity.is_some() {
        return Err(invalid("cannot use --linttool-severity without --linttool"));
    }
    if o.files.len() > 1 && !o.inplace && !o.check {
        return Err(invalid("More than one file given"));
    }
    if !o.files.is_empty() && o.logging_filename.is_some() {
        return Err(invalid(
            "Can't supply both a filename and a filename for logging",
        ));
    }
    if o.range.is_some() && o.line_range.is_some() {
        return Err(invalid("Cannot use --range with --line-range"));
    }
    let range = o.range.is_some() || o.line_range.is_some();
    if o.diff {
        if !o.files.is_empty() || o.logging_filename.is_some() {
            return Err(invalid("--diff mode expects no files"));
        }
        if range {
            return Err(invalid("--diff mode expects no range"));
        }
        if o.at.is_some() {
            return Err(invalid("--diff mode can't format at-char"));
        }
        if o.check {
            return Err(invalid("Can't check formatting in --diff mode"));
        }
    }
    if o.inplace {
        if o.files.is_empty() {
            return Err(invalid("Provide a filename to format in-place"));
        }
        if range {
            return Err(invalid("Can't format a range in-place"));
        }
        if o.at.is_some() {
            return Err(invalid("Can't format at-char in-place"));
        }
    }
    if o.check {
        if o.files.is_empty() {
            return Err(invalid("Provide a filename to check formatting"));
        }
        if o.inplace {
            return Err(invalid("Can't check formatting in-place"));
        }
        if range {
            return Err(invalid("Can't check formatting for a range"));
        }
        if o.at.is_some() {
            return Err(invalid("Can't check formatting at-char"));
        }
    }
    if range && o.at.is_some() {
        return Err(invalid("--at-char expects no range"));
    }
    if !o.inplace && !o.check && o.files.iter().any(|path| Path::new(path).is_dir()) {
        return Err(invalid(
            "Directory inputs require --in-place, --check-formatting, or --linttool",
        ));
    }
    Ok(())
}
type FileIntervals = (String, Vec<crate::engine::Range>);
fn diff_files(text: &str) -> Result<Vec<FileIntervals>, FormatError> {
    let mut result: Vec<FileIntervals> = vec![];
    let mut current = None;
    let mut line = 0;
    let re = regex::Regex::new(r"\+(\d+)").unwrap();
    for s in text.lines() {
        if let Some(file) = s.strip_prefix("+++ ") {
            if file == "/dev/null" {
                current = None;
            } else {
                result.push((file.strip_prefix("b/").unwrap_or(file).into(), vec![]));
                current = Some(result.len() - 1);
            }
            line = 0;
        } else if let Some(current) = current {
            if s.starts_with("@@") {
                line = re
                    .captures(s)
                    .and_then(|c| c[1].parse().ok())
                    .ok_or_else(|| error("Invalid diff: Invalid hunk header", 4))?;
            } else if s.starts_with('+') {
                let intervals = &mut result[current].1;
                if let Some(last) = intervals.last_mut().filter(|(_, e)| *e + 1 >= line) {
                    last.1 = line;
                } else {
                    intervals.push((line, line));
                }
                line += 1;
            } else if !s.starts_with('-') {
                line += 1;
            }
        }
    }
    Ok(result)
}
/// Return the minimal complete-line edit used by the upstream IDE adapter.
pub fn minimal_edit(old: &str, new: &str) -> (usize, usize, String) {
    let a: Vec<_> = old.lines().collect();
    let b: Vec<_> = new.lines().collect();
    let first = a
        .iter()
        .zip(b.iter().map(Some).chain(std::iter::repeat(None)))
        .position(|(x, y)| Some(x) != y);
    let last = a
        .iter()
        .rev()
        .zip(b.iter().rev().map(Some).chain(std::iter::repeat(None)))
        .position(|(x, y)| Some(x) != y);
    match (first, last) {
        (Some(mut start), Some(reverse)) => {
            let end = a.len() - reverse;
            let from = start.min(b.len());
            let to = b.len().saturating_sub(a.len() - end).max(from);
            let novel = &b[from..to];
            let text = if novel.is_empty() && end < a.len() {
                start = start.saturating_sub(1);
                String::new()
            } else {
                novel.join("\n") + "\n"
            };
            (start, end, text)
        }
        _ => (0, 0, String::new()),
    }
}
fn ide(text: &str, formatted: &str) -> String {
    let (start, end, new_text) = minimal_edit(text, formatted.trim_end_matches('\n'));
    let received = text
        .lines()
        .enumerate()
        .map(|(i, s)| format!("{} {s}", i + 1))
        .collect::<Vec<_>>()
        .join("\n");
    let lines: Vec<_> = text.lines().collect();
    let mut updated: Vec<_> = lines[..start].to_vec();
    updated.extend(new_text.lines());
    updated.extend(&lines[end..]);
    let suffix = if updated.last().is_none_or(|s| s.is_empty()) {
        ""
    } else {
        "\n"
    };
    let applied = updated.join("\n") + suffix;
    assert_eq!(
        applied, formatted,
        "IDE edit must reproduce full-file formatting"
    );
    format!(
        "> lines and columns in ranges are 1-indexed\nreceived code:\n{received}\n\nNew text at range {}:1-{}:1 (1-indexed) is\n'{new_text}'\n\nCode after applying language server edits:\n'{applied}'\n\nMatched Hackfmt result\n",
        start + 1,
        end + 1
    )
}
fn formatting_config(o: &Options, root: &Path) -> Config {
    let mut c = config(root);
    if let Some(n) = o.indent {
        c.indent_width = n;
    }
    if let Some(n) = o.width {
        c.line_width = n;
    }
    if o.version.is_some() {
        c.formatting_version = o.version;
    }
    c.tabs |= o.tabs;
    c.format_generated_code |= o.generated;
    c
}

/// Compute a bounded batch concurrently, then consume its results in input
/// order. Workers never print or write files. In particular, a fatal error
/// prevents later results from being applied, just as in the sequential CLI.
fn process_files<T: Sync, R: Send>(
    items: &[T],
    jobs: usize,
    work: impl Fn(&T) -> R + Sync,
    mut consume: impl FnMut(&T, R) -> Result<(), FormatError>,
) -> Result<(), FormatError> {
    #[cfg(not(target_family = "wasm"))]
    if jobs > 1 && items.len() > 1 {
        use rayon::prelude::*;
        let jobs = jobs.min(items.len());
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .thread_name(|i| format!("hakana-format-{i}"))
            .stack_size(8 * 1024 * 1024)
            .build()
            .map_err(|e| invalid(format!("Cannot start formatter workers: {e}")))?;
        // Bound retained source/output buffers as well as the worker count.
        for batch in items.chunks(jobs.saturating_mul(4)) {
            let results = pool.install(|| batch.par_iter().map(&work).collect::<Vec<_>>());
            for (item, result) in batch.iter().zip(results) {
                consume(item, result)?;
            }
        }
        return Ok(());
    }
    // WASM uses the same serial path as --jobs 1.
    let _ = jobs;
    for item in items {
        consume(item, work(item))?;
    }
    Ok(())
}

fn parse_error(path: &str, e: &FormatError) {
    eprintln!(
        "{}\nParse error in file: {path}",
        e.message
            .trim_end_matches("\nFile failed to parse without errors")
    );
}

fn run_options(o: Options) -> Result<(), FormatError> {
    validate(&o)?;
    let root = o
        .root
        .clone()
        .unwrap_or_else(|| root_for(o.files.first().map(String::as_str)));
    if !root.is_dir() {
        return Err(invalid(format!(
            "No such file or directory: {}",
            root.display()
        )));
    }
    let c = formatting_config(&o, &root);
    let jobs = o.jobs.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(8)
    });
    if o.lint {
        let mut paths = vec![];
        for f in o.files.iter().rev() {
            if let Some(file) = f.strip_prefix('@') {
                paths.extend(read(Path::new(file))?.lines().map(str::to_owned));
            } else {
                paths.push(f.clone());
            }
        }
        return process_files(
            &expand_paths(&paths)?,
            jobs,
            |path| {
                if !is_hack(path) {
                    return Ok(None);
                }
                let text = read(Path::new(path))?;
                if text.contains("@noformat") {
                    return Ok(None);
                }
                Ok(crate::format(&text, &c)
                    .ok()
                    .filter(|formatted| *formatted != text)
                    .map(|formatted| (text, formatted)))
            },
            |path, result: Result<_, FormatError>| {
                if let Some((text, formatted)) = result? {
                    let value = serde_json::json!({"path":path,"line":1,"char":1,"code":"HACKFORMAT","severity":o.severity.as_deref().unwrap_or("warning"),"original":text,"replacement":formatted,"name":"format","description":"This file is not formatted. Use `arc f` to keep it formatted."});
                    println!("{value}");
                }
                Ok(())
            },
        );
    }
    if o.diff {
        let input = stdin()?;
        let root = o.diff_root.as_ref().unwrap_or(&root);
        let mut changes = vec![];
        process_files(
            &diff_files(&input)?,
            jobs,
            |(path, intervals)| {
                if !is_hack(path) {
                    return Ok(None);
                }
                let file = root.join(path);
                if !file.is_file() {
                    return Err(error(
                        format!("Invalid diff: No such file or directory: {path}"),
                        4,
                    ));
                }
                let text = read(&file)?;
                Ok(Some((file, crate::format_intervals(&text, &c, intervals))))
            },
            |(path, _), result: Result<_, FormatError>| {
                if let Some((file, formatted)) = result? {
                    match formatted {
                        Ok(out) => changes.push((file, path.clone(), out)),
                        Err(e) if e.exit_code == 2 => parse_error(path, &e),
                        Err(e) => return Err(error(format!("Invalid diff: {}", e.message), 4)),
                    }
                }
                Ok(())
            },
        )?;
        for (file, path, out) in changes {
            if o.dry {
                print!("*** {path}\n{out}");
            } else {
                fs::write(&file, out).map_err(|e| invalid(e.to_string()))?;
            }
        }
        return Ok(());
    }
    if o.inplace || o.check {
        return process_files(
            &expand_paths(&o.files)?,
            jobs,
            |path| {
                if !is_hack(path) {
                    return Ok(None);
                }
                let text = read(Path::new(path))?;
                let formatted = crate::format(&text, &c);
                Ok(Some((text, formatted)))
            },
            |path, result: Result<_, FormatError>| {
                let Some((text, formatted)) = result? else {
                    eprintln!("Unexpected file extension (probably not a Hack file): {path}");
                    return Ok(());
                };
                match formatted {
                    Ok(out) if out != text => {
                        if o.check {
                            println!("{path}");
                        } else {
                            // An earlier write may have changed this file via a
                            // duplicate path, symlink, or hard link. Reformat a
                            // changed source rather than applying stale output.
                            let current = read(Path::new(path))?;
                            let out = if current == text {
                                out
                            } else {
                                match crate::format(&current, &c) {
                                    Ok(out) => out,
                                    Err(e) => {
                                        parse_error(path, &e);
                                        return Ok(());
                                    }
                                }
                            };
                            if out != current {
                                fs::write(path, out).map_err(|e| invalid(e.to_string()))?;
                            }
                        }
                    }
                    Ok(_) => (),
                    Err(e) => parse_error(path, &e),
                }
                Ok(())
            },
        );
    }
    let file = o.files.first();
    if let Some(path) = file.or(o.logging_filename.as_ref())
        && !is_hack(path)
    {
        return Err(invalid(format!(
            "Unexpected file extension (probably not a Hack file): {path}"
        )));
    }
    let text = if let Some(path) = file {
        read(Path::new(path))?
    } else {
        let mut text = stdin()?;
        if !o.raw && !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text
    };
    if o.ide {
        print!("{}", ide(&text, &crate::format(&text, &c)?));
    } else if let Some(offset) = o.at {
        let ((s, e), out) = crate::format_at_offset(&text, &c, offset).map_err(|e| {
            if e.exit_code == 3 {
                invalid(e.message)
            } else {
                e
            }
        })?;
        print!("{s} {e}\n{out}");
    } else if o.range.is_some() || o.line_range.is_some() {
        let range = if let Some((s, e)) = o.line_range {
            ranges::line_range(&text, s.max(1), e.min(ranges::lines(&text).len()))
                .map_err(invalid)?
        } else {
            let (s, e) = o.range.unwrap();
            if o.raw {
                (s, e)
            } else {
                ranges::expand_lines(&text, (s.saturating_sub(1), e.saturating_sub(1)))
            }
        };
        let mut out = if o.raw {
            let (root, _) = crate::syntax::parse(&text, c.enable_xhp_class_modifier);
            let doc = crate::transform::Formatter { env: &c }.t(&root);
            let groups = crate::engine::build(&c, &doc);
            crate::ranges::format_range(&c, &root, &text, &groups, range)
        } else {
            crate::format_range(&text, &c, range.0, range.1.min(text.len()))?
        };
        if !o.raw && !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        print!("{out}");
    } else {
        print!("{}", crate::format(&text, &c)?);
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
pub fn run(args: impl Iterator<Item = String>) -> i32 {
    let o = match options(args) {
        Ok(o) => o,
        Err(e) => {
            let _ = e.print();
            // hackfmt reports invalid arguments with exit code 3.
            return if e.use_stderr() { 3 } else { 0 };
        }
    };
    match run_options(o) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{}", e.message);
            e.exit_code
        }
    }
}
