use hakana_analyzer::config::Config;
use hakana_code_info::code_location::FilePath;
use hakana_code_info::file_info::ParserError;
use hakana_logger::Logger;
use hakana_orchestrator::scanner::get_filesystem;
use hakana_str::{ReflectionInterner, ThreadedInterner};
use indicatif::{ProgressBar, ProgressStyle};
use oxidized::aast::{Def, Expr_, Stmt_};
use oxidized::ast::Pos;
use oxidized::{
    aast,
    aast_visitor::{visit, AstParams, Node, Visitor},
};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
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
    let mut interner = ReflectionInterner::default();
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
        Some(false),
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

    Ok(Arc::try_unwrap(executable_lines)
        .unwrap()
        .into_inner()
        .unwrap())
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
    let interner = interner.parent.lock().unwrap();
    let str_path = interner.lookup(&file_path.0).to_string();

    logger.log_debug_sync(&format!("scanning {}", str_path));
    let aast = hakana_orchestrator::get_aast_for_path(file_path, &str_path);
    let aast = match aast {
        Ok(aast) => aast,
        Err(_) => panic!("invalid file: {}", str_path),
    };
    let mut checker = Scanner {};
    let mut context = BTreeSet::new();
    match visit(&mut checker, &mut context, &aast.0) {
        Ok(_) => ExecutableLines {
            path: file_path.get_relative_path(&interner, root_dir),
            executable_lines: to_ranges(context.clone()),
        },
        Err(_) => panic!("invalid file: {}", str_path),
    }
}

struct Scanner {}

impl<'ast> Visitor<'ast> for Scanner {
    type Params = AstParams<BTreeSet<u64>, ParserError>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params = Self::Params> {
        self
    }

    fn visit_program(
        &mut self,
        c: &mut BTreeSet<u64>,
        p: &aast::Program<(), ()>,
    ) -> Result<(), ParserError> {
        for def in &p.0 {
            match &def {
                Def::Namespace(boxed) => {
                    for ns_def in &boxed.1 {
                        match &ns_def {
                            Def::Fun(boxed) => {
                                self.visit_block(c, &boxed.fun.body.fb_ast)?;
                                ()
                            }
                            Def::Class(boxed) => {
                                for method in &boxed.methods {
                                    self.visit_block(c, &method.body.fb_ast)?;
                                }
                                ()
                            }
                            _ => (),
                        }
                    }
                    ()
                }
                Def::Fun(boxed) => {
                    self.visit_block(c, &boxed.fun.body.fb_ast)?;
                    ()
                }
                Def::Class(boxed) => {
                    for method in &boxed.methods {
                        self.visit_block(c, &method.body.fb_ast)?;
                    }
                    ()
                }
                _ => (),
            }
        }
        Ok(())
    }

    fn visit_stmt(
        &mut self,
        c: &mut BTreeSet<u64>,
        p: &aast::Stmt<(), ()>,
    ) -> Result<(), ParserError> {
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
                push_pos(&boxed.1 .1, c);
                boxed.0.recurse(c, self)
            }
            Stmt_::While(boxed) => {
                push_pos(&boxed.0 .1, c);
                boxed.1.recurse(c, self)
            }
            Stmt_::If(boxed) => {
                push_pos(&boxed.0 .1, c); // if expression
                boxed.1.recurse(c, self)?;
                boxed.2.recurse(c, self)
            }
            Stmt_::Switch(boxed) => {
                push_start(&p.0, c);
                for case_stmt in &boxed.1 {
                    push_pos(&case_stmt.0 .1, c);
                    case_stmt.recurse(c, self)?;
                }
                boxed.2.recurse(c, self)
            }
            Stmt_::Block(boxed) => boxed.recurse(c, self),
            Stmt_::Expr(boxed) => self.visit_expr(c, &boxed),
            Stmt_::Try(boxed) => self.visit_block(c, &boxed.0),
            Stmt_::Concurrent(boxed) => {
                push_start(&p.0, c); // The line where concurrent block is declared is coverable
                self.visit_block(c, boxed)
            }
            Stmt_::Return(boxed) => {
                // a single-line return is always coverable
                if is_single_line(&p.0) {
                    push_pos(&p.0, c);
                    return Ok(());
                }
                match **boxed {
                    None => Ok(()),
                    Some(ref expr) => self.visit_expr(c, &expr),
                }
            }
            _ => {
                let result = p.recurse(c, self);
                push_pos(&p.0, c);
                result
            }
        }
    }

    fn visit_expr(
        &mut self,
        c: &mut BTreeSet<u64>,
        p: &aast::Expr<(), ()>,
    ) -> Result<(), ParserError> {
        match &p.2 {
            Expr_::Efun(boxed) => self.visit_block(c, &boxed.fun.body.fb_ast),
            Expr_::Lfun(boxed) => self.visit_block(c, &boxed.0.body.fb_ast),
            Expr_::Await(boxed) => self.visit_expr(c, boxed),
            Expr_::As(boxed) => self.visit_expr(c, &boxed.expr),
            Expr_::Assign(boxed) => {
                // a single-line assignment is always coverable
                if is_single_line(&boxed.2 .1) {
                    push_pos(&boxed.2 .1, c);
                    return Ok(());
                }
                self.visit_expr(c, &boxed.2)?;
                Ok(())
            }
            Expr_::Shape(vec) => {
                for tuple in vec {
                    // a single-line shape field is always coverable
                    if is_single_line(&tuple.1 .1) {
                        push_pos(&tuple.1 .1, c);
                    }
                }
                push_end(&p.1, c);
                Ok(())
            }
            Expr_::ValCollection(boxed) => {
                for expr in &boxed.2 {
                    self.visit_expr(c, expr)?;
                }
                push_end(&p.1, c);
                Ok(())
            }
            Expr_::KeyValCollection(boxed) => {
                for field in &boxed.2 {
                    self.visit_expr(c, &field.0)?;
                    self.visit_expr(c, &field.1)?;
                }
                push_end(&p.1, c);
                Ok(())
            }
            Expr_::Call(boxed) => {
                // a single-line function call is always coverable
                if is_single_line(&p.1) {
                    push_pos(&p.1, c);
                    return Ok(());
                }

                self.visit_expr(c, &boxed.func)?;
                for arg in &boxed.args {
                    match &arg {
                        aast::Argument::Ainout(_, expr) => {
                            self.visit_expr(c, expr)?;
                        }
                        aast::Argument::Anormal(expr) => {
                            self.visit_expr(c, expr)?;
                        }
                    }
                }
                push_end(&p.1, c);
                Ok(())
            }
            Expr_::Pipe(boxed) => {
                self.visit_expr(c, &boxed.1)?;
                self.visit_expr(c, &boxed.2)?;
                Ok(())
            }
            Expr_::Null
            | Expr_::True
            | Expr_::False
            | Expr_::Int(_)
            | Expr_::Float(_)
            | Expr_::String(_)
            | Expr_::String2(_)
            | Expr_::PrefixedString(_)
            | Expr_::Lvar(_) => {
                push_pos(&p.1, c);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn visit_block(
        &mut self,
        c: &mut BTreeSet<u64>,
        p: &aast::Block<(), ()>,
    ) -> Result<(), ParserError> {
        for stmt in &p.0 {
            self.visit_stmt(c, stmt)?;
        }
        Ok(())
    }
}

fn push_start(p: &Pos, res: &mut BTreeSet<u64>) {
    let start = p.to_raw_span().start.line();
    res.insert(start);
}

fn push_pos(p: &Pos, res: &mut BTreeSet<u64>) {
    let start = p.to_raw_span().start.line();
    let end = p.to_raw_span().end.line();
    if start != 0 && end != 0 {
        for line in start..(end + 1) {
            res.insert(line);
        }
    }
}

fn push_end(p: &Pos, res: &mut BTreeSet<u64>) {
    let end = p.to_raw_span().end.line();
    res.insert(end);
}

fn is_single_line(p: &Pos) -> bool {
    let start = p.to_raw_span().start.line();
    let end = p.to_raw_span().end.line();
    return start == end;
}

// Given an ordered set of ints representing individual executable lines,
// return an ordered vec of strings representing continuous executable ranges.
// For example: [1, 3, 4, 5, 7, 10, 11, 12] -> ["1-1", "3-5", "7-7", "10-12"]
fn to_ranges(lines: BTreeSet<u64>) -> Vec<String> {
    let sorted = Vec::from_iter(lines);
    let mut out = vec![];
    let mut i = 0;
    while let Some(start) = sorted.get(i) {
        i += 1;
        out.push(make_range(&sorted, &start, &mut i));
    }
    out
}

// Create a single range where the first executable line number is `start`
// and `i` is the index just *after* that line. Update `i` as we go.
fn make_range(sorted: &Vec<u64>, start: &u64, i: &mut usize) -> String {
    let mut end = start;

    while let Some(curr) = sorted.get(*i) {
        if *curr == end + 1 {
            end = curr;
            *i += 1;
        } else {
            break;
        }
    }

    format!("{}-{}", start, end)
}
