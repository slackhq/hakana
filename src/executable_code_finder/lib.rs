use hakana_analyzer::config::Config;
use hakana_logger::Logger;
use hakana_code_info::code_location::FilePath;
use hakana_code_info::file_info::ParserError;
use hakana_str::{Interner, ThreadedInterner};
use hakana_workhorse::file::VirtualFileSystem;
use hakana_workhorse::scanner::add_builtins_to_scan;
use indicatif::{ProgressBar, ProgressStyle};
use oxidized::aast::Stmt_;
use oxidized::ast::Pos;
use oxidized::{aast, aast_visitor::{visit, AstParams, Node, Visitor}};
use rustc_hash::FxHashMap;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Serialize)]
pub struct ExecutableLines {
    pub path: String,
    pub executable_lines: Vec<String>,
}

pub fn scan_files(
    scan_dirs: &Vec<String>,
    cache_dir: Option<&String>,
    config: &Arc<Config>,
    threads: u8,
    logger: Arc<Logger>,
) -> Result<Vec<ExecutableLines>, ()> {
    logger.log_debug_sync(&format!("{:#?}", scan_dirs));

    let mut files_to_scan = vec![];
    let mut files_to_analyze = vec![];
    let mut interner = Interner::default();
    let existing_file_system = None;

    get_filesystem(
        &mut files_to_scan,
        &mut interner,
        &logger,
        scan_dirs,
        &existing_file_system,
        config,
        cache_dir,
        &mut files_to_analyze,
    );

    let executable_lines = Arc::new(Mutex::new(vec![]));

    if !files_to_scan.is_empty() {
        let file_scanning_now = Instant::now();

        let bar = if logger.show_progress() {
            let pb = ProgressBar::new(files_to_scan.len() as u64);
            let sty =
                ProgressStyle::with_template("{bar:40.green/yellow} {pos:>7}/{len:7}").unwrap();
            pb.set_style(sty);
            Some(Arc::new(pb))
        } else {
            None
        };

        let files_processed: Arc<Mutex<u64>> = Arc::new(Mutex::new(0));

        let mut group_size = threads as usize;
        let mut path_groups = FxHashMap::default();
        if files_to_scan.len() < 4 * group_size {
            group_size = 1;
        }

        for (i, str_path) in files_to_scan.into_iter().enumerate() {
            let group = i % group_size;
            path_groups
                .entry(group)
                .or_insert_with(Vec::new)
                .push(FilePath(interner.get(str_path.as_str()).unwrap()));
        }

        let interner = Arc::new(Mutex::new(interner));
        let mut handles = vec![];

        for (_, path_group) in path_groups {
            let interner = interner.clone();
            let bar = bar.clone();
            let files_processed = files_processed.clone();
            let logger = logger.clone();
            let executable_lines = executable_lines.clone();
            let root_dir = config.root_dir.clone();

            let handle = std::thread::spawn(move || {
                let new_interner = ThreadedInterner::new(interner);

                for file_path in &path_group {
                    let res = scan_file(&new_interner, &root_dir, *file_path, &logger.clone());
                    let mut executable_lines = executable_lines.lock().unwrap();
                    if !res.executable_lines.is_empty() {
                        executable_lines.push(res);
                    }
                    let mut tally = files_processed.lock().unwrap();
                    *tally += 1;
                    update_progressbar(*tally, bar.clone());
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        if let Some(bar) = &bar {
            bar.finish_and_clear();
        }

        if logger.can_log_timing() {
            logger.log_sync(&format!(
                "Scanning files took {:.2?}",
                file_scanning_now.elapsed()
            ));
        }
    }

    Ok(Arc::try_unwrap(executable_lines).unwrap().into_inner().unwrap())
}

fn get_filesystem(
    files_to_scan: &mut Vec<String>,
    interner: &mut Interner,
    logger: &Logger,
    scan_dirs: &Vec<String>,
    existing_file_system: &Option<VirtualFileSystem>,
    config: &Arc<Config>,
    cache_dir: Option<&String>,
    files_to_analyze: &mut Vec<String>,
) -> VirtualFileSystem {
    let mut file_system = VirtualFileSystem::default();

    add_builtins_to_scan(files_to_scan, interner, &mut file_system);

    logger.log_sync("Looking for Hack files");

    for scan_dir in scan_dirs {
        logger.log_debug_sync(&format!(" - in {}", scan_dir));

        files_to_scan.extend(file_system.find_files_in_dir(
            scan_dir,
            interner,
            existing_file_system,
            config,
            cache_dir.is_some() || config.ast_diff,
            files_to_analyze,
        ));
    }

    file_system
}


fn update_progressbar(percentage: u64, bar: Option<Arc<ProgressBar>>) {
    if let Some(bar) = bar {
        bar.set_position(percentage);
    }
}

pub(crate) fn scan_file(
    interner: &ThreadedInterner,
    root_dir: &str,
    file_path: FilePath,
    logger: &Logger,
) -> ExecutableLines {
    let interner = interner
        .parent
        .lock()
        .unwrap();
    let str_path = interner
        .lookup(&file_path.0)
        .to_string();

    logger.log_debug_sync(&format!("scanning {}", str_path));
    let aast = hakana_workhorse::get_aast_for_path(file_path, &str_path);
    let aast = match aast {
        Ok(aast) => aast,
        Err(_) => panic!("invalid file: {}", str_path)
    };
    let mut checker = Scanner {};
    let mut context = Vec::new();
    match visit(&mut checker, &mut context, &aast.0) {
        Ok(_) => ExecutableLines {
            path: file_path.get_relative_path(&interner, root_dir),
            executable_lines: context,
        },
        Err(_) => panic!("invalid file: {}", str_path)
    }
}

struct Scanner {}

impl<'ast> Visitor<'ast> for Scanner {
    type Params = AstParams<Vec<String>, ParserError>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params=Self::Params> {
        self
    }

    fn visit_stmt(&mut self, c: &mut Vec<String>, p: &aast::Stmt<(), ()>) -> Result<(), ParserError> {
        match &p.1 {
            Stmt_::For(boxed) => {
                push_start(&p.0, c); // The line where for loop is declared is coverable
                boxed.1.recurse(c, self)
            }
            Stmt_::Foreach(boxed) => {
                push_start(&p.0, c); // The line where foreach loop is declared is coverable
                boxed.2.recurse(c, self)
            }
            Stmt_::Do(boxed) => {
                push_pos(&boxed.1.1, c);
                boxed.0.recurse(c, self)
            }
            Stmt_::While(boxed) => {
                push_pos(&boxed.0.1, c);
                boxed.1.recurse(c, self)
            }
            Stmt_::If(boxed) => {
                push_pos(&boxed.0.1, c); // if expression
                boxed.1.recurse(c, self)?;
                boxed.2.recurse(c, self)
            }
            Stmt_::Switch(boxed) => {
                // Skipping the switch statement, it's never covered by HHVM
                for case_stmt in &boxed.1 {
                    push_pos(&case_stmt.0.1, c);
                    case_stmt.recurse(c, self)?;
                }
                boxed.2.recurse(c, self)
            }
            Stmt_::Block(boxed) => {
                boxed.recurse(c, self)
            }
            Stmt_::Expr(boxed) => {
                let start = boxed.1.to_raw_span().start.line();
                let end = boxed.1.to_raw_span().end.line();
                if start == end {
                    c.push(format!("{}-{}", start, end));
                } else {
                    // Multi-line expressions seem to miss the first line in HHVM coverage
                    c.push(format!("{}-{}", start + 1, end));
                }
                Ok(())
            }
            _ => {
                let result = p.recurse(c, self);
                push_pos(&p.0, c);
                result
            }
        }
    }
}

fn push_start(p: &Pos, res: &mut Vec<String>) {
    let start = p.to_raw_span().start.line();
    res.push(format!("{}-{}", start, start));
}

fn push_pos(p: &Pos, res: &mut Vec<String>) {
    let start = p.to_raw_span().start.line();
    let end = p.to_raw_span().end.line();
    if start != 0 && end != 0 {
        res.push(format!("{}-{}", start, end));
    }
}