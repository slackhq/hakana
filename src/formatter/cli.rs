//! Command-line compatibility with hackfmt, shared by `hakana format` and the
//! small formatter development binary. Formatting never initializes analysis.
use crate::{Config, FormatError, ranges};
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
            p.parent().unwrap_or(&cwd).to_path_buf()
        })
        .unwrap_or_else(|| cwd.clone());
    for p in start.ancestors() {
        if p.join(".hhconfig").is_file() {
            return p.into();
        }
    }
    cwd
}
fn is_hack(path: &str) -> bool {
    matches!(
        Path::new(path).extension().and_then(|s| s.to_str()),
        Some("php" | "hack" | "hhi" | "hh")
    )
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
}
fn options(args: impl Iterator<Item = String>) -> Result<Options, FormatError> {
    let mut o = Options::default();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| invalid(format!("Missing value for {arg}")))
        };
        match arg.as_str() {
            "--test" => (),
            "--root" => o.root = Some(value()?.into()),
            "--diff-root" => o.diff_root = Some(value()?.into()),
            "--filename-for-logging" => o.logging_filename = Some(value()?),
            "--range" | "--line-range" | "--test-raw-range" => {
                let a = value()?
                    .parse::<usize>()
                    .map_err(|_| invalid("Invalid range"))?;
                let b = value()?
                    .parse::<usize>()
                    .map_err(|_| invalid("Invalid range"))?;
                if arg == "--line-range" {
                    o.line_range = Some((a, b));
                } else {
                    o.range = Some((a, b));
                    o.raw = arg == "--test-raw-range";
                }
            }
            "--at-char" => o.at = Some(value()?.parse().map_err(|_| invalid("Invalid offset"))?),
            "--indent-width" => {
                o.indent = Some(
                    value()?
                        .parse()
                        .map_err(|_| invalid("Invalid indent width"))?,
                )
            }
            "--line-width" => {
                o.width = Some(
                    value()?
                        .parse()
                        .map_err(|_| invalid("Invalid line width"))?,
                )
            }
            "--version" => {
                o.version = Some(value()?.parse().map_err(|_| invalid("Invalid version"))?)
            }
            "--tabs" => o.tabs = true,
            "--format-generated-code" => o.generated = true,
            "-i" | "--in-place" => o.inplace = true,
            "--check-formatting" | "--check" => o.check = true,
            "--diff" => o.diff = true,
            "--diff-dry-run" => {
                o.diff = true;
                o.dry = true;
            }
            "--linttool" => o.lint = true,
            "--linttool-severity" => o.severity = Some(value()?),
            "--test-ide" => o.ide = true,
            "--" => {
                o.files.extend(args);
                break;
            }
            "-" => (),
            x if x.starts_with('-') => return Err(invalid(format!("Unknown option {x}"))),
            _ => o.files.push(arg),
        }
    }
    Ok(o)
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
    if o.lint {
        let mut paths = vec![];
        for f in o.files.iter().rev() {
            if let Some(file) = f.strip_prefix('@') {
                paths.extend(read(Path::new(file))?.lines().map(str::to_owned));
            } else {
                paths.push(f.clone());
            }
        }
        for path in paths {
            if !is_hack(&path) {
                continue;
            }
            let text = read(Path::new(&path))?;
            if text.contains("@noformat") {
                continue;
            }
            if let Ok(formatted) = crate::format(&text, &c)
                && formatted != text
            {
                let value = serde_json::json!({"path":path,"line":1,"char":1,"code":"HACKFORMAT","severity":o.severity.as_deref().unwrap_or("warning"),"original":text,"replacement":formatted,"name":"format","description":"This file is not formatted. Use `arc f` to keep it formatted."});
                println!("{value}");
            }
        }
        return Ok(());
    }
    if o.diff {
        let input = stdin()?;
        let root = o.diff_root.as_ref().unwrap_or(&root);
        let mut changes = vec![];
        for (path, intervals) in diff_files(&input)? {
            if !is_hack(&path) {
                continue;
            }
            let file = root.join(&path);
            if !file.is_file() {
                return Err(error(
                    format!("Invalid diff: No such file or directory: {path}"),
                    4,
                ));
            }
            let text = read(&file)?;
            match crate::format_intervals(&text, &c, &intervals) {
                Ok(out) => changes.push((file, path, out)),
                Err(e) if e.exit_code == 2 => eprintln!(
                    "{}\nParse error in file: {path}",
                    e.message
                        .trim_end_matches("\nFile failed to parse without errors")
                ),
                Err(e) => return Err(error(format!("Invalid diff: {}", e.message), 4)),
            }
        }
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
        for path in &o.files {
            if !is_hack(path) {
                eprintln!("Unexpected file extension (probably not a Hack file): {path}");
                continue;
            }
            let text = read(Path::new(path))?;
            match crate::format(&text, &c) {
                Ok(out) if out != text => {
                    if o.check {
                        println!("{path}");
                    } else {
                        fs::write(path, out).map_err(|e| invalid(e.to_string()))?;
                    }
                }
                Ok(_) => (),
                Err(e) => {
                    eprintln!(
                        "{}\nParse error in file: {path}",
                        e.message
                            .trim_end_matches("\nFile failed to parse without errors")
                    );
                }
            }
        }
        return Ok(());
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
        if !o.raw && !out.ends_with('\n') {
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
    let args: Vec<_> = args.collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "Usage: hakana format [OPTIONS] [FILE ...]\n\nFormat Hack source; reads stdin when FILE is omitted.\n\n  -i, --in-place              Update files in place\n  --check-formatting         Print files that need formatting\n  --range START END          Format a 1-based byte range (end exclusive)\n  --line-range START END     Format an inclusive line range\n  --at-char OFFSET           Format at a 0-based byte offset\n  --indent-width N           Spaces per indentation level (default 2)\n  --line-width N             Maximum line width (default 80)\n  --tabs                     Indent with tabs\n  --version N                Select hackfmt formatting rules\n  --format-generated-code   Format generated sections\n  --root DIRECTORY          Directory containing .hhconfig\n  --diff, --diff-dry-run     Format changed lines from a diff on stdin"
        );
        return 0;
    }
    match options(args.into_iter()).and_then(run_options) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{}", e.message);
            e.exit_code
        }
    }
}
