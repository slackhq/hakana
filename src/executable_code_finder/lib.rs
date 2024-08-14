use std::sync::{Arc, Mutex};
use std::time::Instant;
use hakana_analyzer::config::Config;
use hakana_logger::Logger;
use hakana_reflection_info::code_location::FilePath;
use hakana_str::{Interner, ThreadedInterner};
use hakana_workhorse::file::{VirtualFileSystem};
use hakana_workhorse::scanner::{add_builtins_to_scan};
use indicatif::{ProgressBar, ProgressStyle};
use oxidized::{aast, aast_visitor::{visit, AstParams, Node, Visitor}};
use rustc_hash::FxHashMap;
use hakana_reflection_info::file_info::ParserError;

struct Context {
}

pub fn scan_files(
    scan_dirs: &Vec<String>,
    cache_dir: Option<&String>,
    config: &Arc<Config>,
    threads: u8,
    logger: Arc<Logger>,
) -> Result<(),()> {
    logger.log_debug_sync(&format!("{:#?}", scan_dirs));

    let mut files_to_scan = vec![];
    let mut files_to_analyze = vec![];
    let mut interner=  Interner::default();
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

    let invalid_files = Arc::new(Mutex::new(vec![]));

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
            let invalid_files = invalid_files.clone();

            let handle = std::thread::spawn(move || {
                let mut new_context = Context {};
                let new_interner = ThreadedInterner::new(interner);

                for file_path in &path_group {
                    let str_path = new_interner
                        .parent
                        .lock()
                        .unwrap()
                        .lookup(&file_path.0)
                        .to_string();

                    println!("{}", str_path);

                    match scan_file(&str_path, *file_path, &mut new_context, &logger.clone(), ) {
                        Err(_) => {
                            invalid_files.lock().unwrap().push(*file_path);
                        }
                        Ok(_) => {}
                    };

                    let mut tally = files_processed.lock().unwrap();
                    *tally += 1;

                    update_progressbar(*tally, bar.clone());
                }

                //resolved_names.lock().unwrap().extend(local_resolved_names);

                //let mut codebases = codebases.lock().unwrap();
                //codebases.push(new_codebase);
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

    let _invalid_files = Arc::try_unwrap(invalid_files)
        .unwrap()
        .into_inner()
        .unwrap();

    Ok(())
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
    str_path: &str,
    file_path: FilePath,
    context: &mut Context,
    logger: &Logger,
) -> Result<(), ParserError>{
    logger.log_debug_sync(&format!("scanning {}", str_path));

    let aast = hakana_workhorse::get_aast_for_path(file_path, str_path);

    let aast = match aast {
        Ok(aast) => aast,
        Err(err) => {
            return Err(err);
        }
    };

    let mut checker = Scanner {
    };

    visit(&mut checker, context, &aast.0)
}

struct Scanner {

}

impl<'ast> Visitor<'ast> for Scanner {
    type Params = AstParams<Context, ParserError>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params = Self::Params> {
        self
    }

    fn visit_stmt(&mut self, c: &mut Context, p: &aast::Stmt<(), ()>) -> Result<(), ParserError> {
        let result = p.recurse(c, self);

        //println!("{}-{}", p.0.to_raw_span().start.line(),p.0.to_raw_span().end.line());

        result
    }
}