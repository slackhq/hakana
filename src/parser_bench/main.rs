//! Parse-only benchmark comparing the HHVM pipeline with Hakana's own parser.
//!
//! Usage: parse-bench <mode> <file-list> [--threads=N] [--count-allocs]
//!
//! `<file-list>` is a text file with one path per line.
//!
//! `parse-bench dump <file>` shows what both parsers make of one file: the
//! errors each reports, every error of the parsers proper, whether the
//! lossless trees agree, and HHVM's tree (with `CST_DUMP=1` the native one
//! too when they differ).

use std::alloc::{GlobalAlloc, Layout};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

struct CountingAlloc;

static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static ALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static PEAK_BYTES: AtomicU64 = AtomicU64::new(0);

fn track_alloc(size: usize) {
    ALLOC_BYTES.fetch_add(size as u64, Ordering::Relaxed);
    ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
    let live = LIVE_BYTES.fetch_add(size as u64, Ordering::Relaxed) + size as u64;
    PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
}
static ARENA_BYTES: AtomicU64 = AtomicU64::new(0);

// What the `readonly` modes saw: trees analysed, those of files (or variants)
// that use `readonly` where HHVM cares, those the analysis reports errors
// for, the errors in total, and the trees the two analyses disagree on.
static READONLY_TREES: AtomicU64 = AtomicU64::new(0);
static READONLY_USERS: AtomicU64 = AtomicU64::new(0);
static READONLY_ERROR_TREES: AtomicU64 = AtomicU64::new(0);
static READONLY_ERRORS: AtomicU64 = AtomicU64::new(0);
static READONLY_DIFFERENCES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            track_alloc(layout.size());
        }
        unsafe { mimalloc::MiMalloc.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if COUNTING.load(Ordering::Relaxed) {
            LIVE_BYTES.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        }
        unsafe { mimalloc::MiMalloc.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            LIVE_BYTES.fetch_sub(layout.size() as u64, Ordering::Relaxed);
            track_alloc(new_size);
        }
        unsafe { mimalloc::MiMalloc.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Read,
    HhvmCst,
    HhvmAast,
    HhvmLower,
    Native,
    /// The whole native pipeline: parse, syntax checks, lowering to the
    /// analysis tree, readonly analysis. The counterpart of `hhvm-aast`.
    NativeAast,
    /// Native parse recording the lossless tree. The counterpart of `hhvm-cst`.
    NativeCst,
    Bridge,
    Diff,
    Errors,
    Readonly,
    ReadonlyInjected,
    Cst,
}

/// How the parsers are set up for the modes that deal with the lossless
/// tree: the way Hakana's analysis has them, or else with
/// `XHP_CLASS_MODIFIER=0` and/or `DISABLE_XHP_CHILDREN=1` in the environment.
fn cst_options() -> hakana_parser::ParserOptions {
    static OPTIONS: std::sync::OnceLock<hakana_parser::ParserOptions> = std::sync::OnceLock::new();
    *OPTIONS.get_or_init(|| hakana_parser::ParserOptions {
        enable_xhp_class_modifier: std::env::var("XHP_CLASS_MODIFIER").as_deref() != Ok("0"),
        disable_xhp_children_declarations: std::env::var("DISABLE_XHP_CHILDREN").as_deref()
            == Ok("1"),
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "usage: parse-bench <read|hhvm-cst|hhvm-lower|hhvm-aast|native|bridge|diff|errors|readonly|readonly-injected|cst|mem> <file-list> [--threads=N] [--count-allocs]"
        );
        std::process::exit(2);
    }
    let mode = match args[1].as_str() {
        "mem" => Mode::Read,
        "read" => Mode::Read,
        "hhvm-cst" => Mode::HhvmCst,
        "hhvm-aast" => Mode::HhvmAast,
        "hhvm-lower" => Mode::HhvmLower,
        "native" => Mode::Native,
        "native-aast" => Mode::NativeAast,
        "native-cst" => Mode::NativeCst,
        "bridge" => Mode::Bridge,
        "diff" => Mode::Diff,
        "errors" => Mode::Errors,
        "readonly" => Mode::Readonly,
        "readonly-injected" => Mode::ReadonlyInjected,
        "cst" => Mode::Cst,
        "dump" => Mode::Read,
        other => panic!("unknown mode {other}"),
    };
    let mut threads = 8usize;
    for a in &args[3..] {
        if let Some(n) = a.strip_prefix("--threads=") {
            threads = n.parse().unwrap();
        } else if a == "--count-allocs" {
            COUNTING.store(true, Ordering::Relaxed);
        }
    }

    if args[1] == "mem" {
        return memory_report(&args[2]);
    }
    if args[1] == "dump" {
        return dump(&args[2]);
    }

    let files: Arc<Vec<String>> = Arc::new(
        std::fs::read_to_string(&args[2])
            .unwrap()
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect(),
    );

    let next = Arc::new(AtomicUsize::new(0));
    let bytes = Arc::new(AtomicU64::new(0));
    let failures = Arc::new(AtomicU64::new(0));
    // Files that are not UTF-8 (or cannot be read at all): the native
    // parser is never given those, so there is nothing to run or compare.
    let unreadable = Arc::new(AtomicU64::new(0));
    let cpu_nanos = Arc::new(AtomicU64::new(0));

    let start = Instant::now();
    let mut handles = vec![];
    for _ in 0..threads {
        let files = files.clone();
        let next = next.clone();
        let bytes = bytes.clone();
        let failures = failures.clone();
        let unreadable = unreadable.clone();
        let cpu_nanos = cpu_nanos.clone();
        handles.push(
            std::thread::Builder::new()
                .stack_size(64 * 1024 * 1024)
                .spawn(move || {
                    let mut local_bytes = 0u64;
                    let mut local_failures = 0u64;
                    let mut arena = bumpalo::Bump::new();
                    let mut parse_time = std::time::Duration::ZERO;
                    while let Some(path) = files.get(next.fetch_add(1, Ordering::Relaxed)) {
                        let Ok(contents) = std::fs::read_to_string(path) else {
                            unreadable.fetch_add(1, Ordering::Relaxed);
                            if std::env::var_os("SHOW_FAILURES").is_some() {
                                eprintln!("UNREADABLE {path}: not UTF-8");
                            }
                            continue;
                        };
                        local_bytes += contents.len() as u64;
                        let t = Instant::now();
                        let ok = if mode == Mode::Native {
                            arena.reset();
                            match hakana_parser::parse(&arena, &contents) {
                                Ok(out) => {
                                    std::hint::black_box(&out);
                                    ARENA_BYTES.fetch_add(
                                        arena.allocated_bytes() as u64,
                                        Ordering::Relaxed,
                                    );
                                    true
                                }
                                Err(e) => {
                                    if std::env::var_os("SHOW_FAILURES").is_some() {
                                        let line =
                                            contents[..e.offset as usize].matches('\n').count() + 1;
                                        eprintln!("FAIL {}:{}: {}", path, line, e.message);
                                    }
                                    false
                                }
                            }
                        } else if mode == Mode::NativeAast {
                            arena.reset();

                            match hakana_parser::parse_file(&arena, &contents, path) {
                                Ok(out) => {
                                    std::hint::black_box(&out);
                                    out.errors.is_empty()
                                }
                                Err(_) => false,
                            }
                        } else if mode == Mode::NativeCst {
                            match hakana_parser::parse_cst_named(&contents, path, cst_options()) {
                                Ok(out) => {
                                    std::hint::black_box(&out);
                                    out.errors.is_empty()
                                }
                                Err(_) => false,
                            }
                        } else {
                            // HHVM's parser panics on a few pathological
                            // inputs; count those as failures and carry on.
                            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                run(mode, path, contents)
                            })) {
                                Ok(ok) => ok,
                                Err(_) => {
                                    eprintln!("PANIC {}", path);
                                    false
                                }
                            }
                        };
                        if !ok {
                            local_failures += 1;
                        }
                        parse_time += t.elapsed();
                    }
                    bytes.fetch_add(local_bytes, Ordering::Relaxed);
                    failures.fetch_add(local_failures, Ordering::Relaxed);
                    cpu_nanos.fetch_add(parse_time.as_nanos() as u64, Ordering::Relaxed);
                })
                .unwrap(),
        );
    }
    for h in handles {
        h.join().unwrap();
    }
    let wall = start.elapsed();
    let mb = bytes.load(Ordering::Relaxed) as f64 / 1_000_000.0;
    let parse_secs = cpu_nanos.load(Ordering::Relaxed) as f64 / 1e9;
    let unreadable = match unreadable.load(Ordering::Relaxed) {
        0 => String::new(),
        n => format!(" unreadable={n}"),
    };
    println!(
        "mode={} files={} MB={:.1} threads={} wall={:.2}s parse_thread_time={:.2}s ({:.1} MB/s/thread) failures={}{unreadable}",
        args[1],
        files.len(),
        mb,
        threads,
        wall.as_secs_f64(),
        parse_secs,
        if parse_secs > 0.0 {
            mb / parse_secs
        } else {
            0.0
        },
        failures.load(Ordering::Relaxed),
    );
    let arena_bytes = ARENA_BYTES.load(Ordering::Relaxed);
    if arena_bytes > 0 {
        println!(
            "arena: {:.1} MB of syntax tree in total ({:.2}x source size)",
            arena_bytes as f64 / 1e6,
            arena_bytes as f64 / 1e6 / mb
        );
    }
    if matches!(mode, Mode::Readonly | Mode::ReadonlyInjected) {
        println!(
            "readonly analysis: {} trees compared; {} use readonly; {} with errors ({} errors); {} differences",
            READONLY_TREES.load(Ordering::Relaxed),
            READONLY_USERS.load(Ordering::Relaxed),
            READONLY_ERROR_TREES.load(Ordering::Relaxed),
            READONLY_ERRORS.load(Ordering::Relaxed),
            READONLY_DIFFERENCES.load(Ordering::Relaxed),
        );
    }
    if COUNTING.load(Ordering::Relaxed) {
        let b = ALLOC_BYTES.load(Ordering::Relaxed) as f64;
        println!(
            "heap: {:.1} MB requested in {} calls ({:.1}x source size)",
            b / 1_000_000.0,
            ALLOC_CALLS.load(Ordering::Relaxed),
            b / 1_000_000.0 / mb,
        );
    }
}

fn run(mode: Mode, path: &str, contents: String) -> bool {
    match mode {
        Mode::Read | Mode::Native | Mode::NativeAast | Mode::NativeCst => true,
        Mode::Cst => {
            use hakana_parser_bench::CstOutcome;
            let show = std::env::var_os("SHOW_FAILURES").is_some();
            match hakana_parser_bench::compare_cst_with(path, &contents, cst_options()) {
                CstOutcome::Same => true,
                // Counted apart from real differences: set CST_STRICT=1 to
                // make files the native parser cannot recover from fail too.
                CstOutcome::NoRecovery(why) => {
                    if std::env::var_os("CST_STRICT").is_some() {
                        if show {
                            eprintln!("NORECOVERY {path}: {why}");
                        }
                        false
                    } else {
                        true
                    }
                }
                CstOutcome::NativeFailed(why) => {
                    if show {
                        eprintln!("FAILED {path}: {why}");
                    }
                    false
                }
                CstOutcome::Different(difference) => {
                    if show {
                        eprintln!("DIFF {path}: {difference}");
                    }
                    // With CST_DUMP=1, print both trees of the first file that differs.
                    if std::env::var_os("CST_DUMP").is_some() {
                        static DUMPED: std::sync::atomic::AtomicBool =
                            std::sync::atomic::AtomicBool::new(false);
                        if !DUMPED.swap(true, Ordering::Relaxed) {
                            let options = cst_options();
                            let (hhvm, _) =
                                hakana_parser_bench::hhvm_cst_with(path, &contents, options);
                            let native = hakana_parser::parse_cst_named(&contents, path, options)
                                .unwrap()
                                .root;
                            let _ = std::fs::write("/tmp/cst.native.txt", native.dump(&contents));
                            let _ = std::fs::write("/tmp/cst.hhvm.txt", hhvm.dump(&contents));
                            eprintln!(
                                "dumped /tmp/cst.native.txt and /tmp/cst.hhvm.txt for {path}"
                            );
                        }
                    }
                    false
                }
            }
        }
        Mode::HhvmCst => {
            let arena = bumpalo::Bump::new();
            let rel = Arc::new(relative_path::RelativePath::make(
                relative_path::Prefix::Root,
                std::path::PathBuf::from(path),
            ));
            let source = parser_core_types::source_text::SourceText::make(rel, contents.as_bytes());
            let env = parser_core_types::parser_env::ParserEnv {
                enable_xhp_class_modifier: true,
                ..Default::default()
            };
            let (root, errors, _state) =
                positioned_by_ref_parser::parse_script(&arena, &source, env);
            std::hint::black_box(&root);
            errors.is_empty()
        }
        Mode::HhvmLower => {
            // CST parse + lowering only: no syntax-error passes, no AST checks.
            let arena = bumpalo::Bump::new();
            let rel = Arc::new(relative_path::RelativePath::make(
                relative_path::Prefix::Root,
                std::path::PathBuf::from(path),
            ));
            let source = parser_core_types::source_text::SourceText::make(rel, contents.as_bytes());
            let indexed =
                parser_core_types::indexed_source_text::IndexedSourceText::new(source.clone());
            let env = parser_core_types::parser_env::ParserEnv {
                enable_xhp_class_modifier: true,
                ..Default::default()
            };
            let (root, errors, _state) =
                positioned_by_ref_parser::parse_script(&arena, &source, env);
            let opts = oxidized::parser_options::ParserOptions {
                enable_xhp_class_modifier: true,
                allow_unstable_features: true,
                ..Default::default()
            };
            let ns = Arc::new(oxidized::namespace_env::Env::empty(
                opts.auto_namespace_map.clone(),
                oxidized::namespace_env::Mode::ForTypecheck,
                false,
            ));
            let mut lenv = lowerer::Env::make(
                oxidized::namespace_env::Mode::ForTypecheck,
                false,
                false,
                oxidized::file_info::Mode::Mstrict,
                hash::HashSet::default(),
                &indexed,
                &opts,
                ns,
                parser_core_types::syntax_by_ref::positioned_token::TokenFactory::new(&arena),
                &arena,
            );
            let aast = lowerer::lower(&mut lenv, &root);
            std::hint::black_box(&aast);
            errors.is_empty()
        }
        Mode::Bridge => match hakana_parser_bench::native(path, &contents) {
            hakana_parser_bench::Native::Parsed { program, .. } => {
                std::hint::black_box(&program);
                true
            }
            hakana_parser_bench::Native::Failed(_) => false,
        },
        Mode::Errors => errors(path, &contents),
        Mode::Readonly => readonly(path, &contents, None),
        Mode::ReadonlyInjected => {
            let mut same = readonly(path, &contents, None);
            let variants = hakana_parser_bench::readonly_variants(&contents);
            for (n, variant) in variants.iter().enumerate() {
                same &= readonly(path, variant, Some(n));
            }
            same
        }
        Mode::Diff => diff(path, contents),
        Mode::HhvmAast => match hakana_parser_bench::hhvm_unmodified(path, &contents) {
            Ok(r) => {
                std::hint::black_box(&r);
                r.2.is_empty()
            }
            Err(_) => false,
        },
    }
}

static DIFF_DUMPS: AtomicUsize = AtomicUsize::new(0);

/// Compares the two parsers on one file, reporting and optionally dumping
/// any disagreement. Returns `true` when they agree.
fn diff(path: &str, contents: String) -> bool {
    use hakana_parser_bench::{Outcome, compare};
    let report = |what: &str| {
        if std::env::var_os("SHOW_FAILURES").is_some() {
            eprintln!("DIFF {}: {}", path, what);
        }
    };
    match compare(path, contents) {
        Outcome::Same | Outcome::BothReject => true,
        Outcome::NativeDeclined => {
            report("native parser declined a file HHVM accepts");
            false
        }
        Outcome::HhvmRejects => {
            report("native parser accepted a file HHVM rejects");
            false
        }
        Outcome::NativeReportsErrors(first) => {
            report(&format!(
                "native parser reports an error HHVM does not: {}",
                first
            ));
            false
        }
        Outcome::HhvmReportsErrors(first) => {
            report(&format!(
                "native parser accepted a file HHVM reports errors for: {}",
                first
            ));
            false
        }
        Outcome::Mismatch { what, native, hhvm } => {
            let n = DIFF_DUMPS.fetch_add(1, Ordering::Relaxed);
            let limit: usize = std::env::var("DIFF_DUMP_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            if n < limit {
                let dir =
                    std::env::var("DIFF_DUMP_DIR").unwrap_or_else(|_| "/tmp/parse-diff".into());
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::write(format!("{}/{}.native.txt", dir, n), native());
                let _ = std::fs::write(format!("{}/{}.hhvm.txt", dir, n), hhvm());
                let _ = std::fs::write(format!("{}/{}.path.txt", dir, n), path);
            }
            report(&format!("mismatch in {}", what));
            false
        }
    }
}

/// Measures, file by file on one thread, how much memory each representation
/// of a file's syntax keeps alive and how much is needed while building it.
fn memory_report(list: &str) {
    let files: Vec<String> = std::fs::read_to_string(list)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();
    COUNTING.store(true, Ordering::Relaxed);
    let live = || LIVE_BYTES.load(Ordering::Relaxed);
    let reset_peak = || PEAK_BYTES.store(live(), Ordering::Relaxed);
    let peak = || PEAK_BYTES.load(Ordering::Relaxed);
    let calls = || ALLOC_CALLS.load(Ordering::Relaxed);

    #[derive(Default)]
    struct Acc {
        retained: u64,
        peak: u64,
        max_peak: u64,
        calls: u64,
    }
    let (mut hhvm, mut bridge, mut native) = (Acc::default(), Acc::default(), Acc::default());
    let mut native_aast = Acc::default();
    let mut source = 0u64;
    let mut counted = 0u64;

    for path in &files {
        let Ok(contents) = std::fs::read_to_string(path) else {
            continue;
        };
        if path.ends_with(".hhi") {
            continue;
        }
        // Native tree: a fresh arena so its size is exactly this file's.
        let (base, c0) = (live(), calls());
        reset_peak();
        let mut arena = bumpalo::Bump::new();
        let ok = match hakana_parser::parse(&arena, &contents) {
            Ok(out) => {
                let tree: u64 = live() - base;
                std::hint::black_box(&out);
                drop(out);
                let used: usize = arena.iter_allocated_chunks().map(|c| c.len()).sum();
                // Chunk capacity is what the allocator hands out; `used` is
                // what the tree occupies. Report the tree itself.
                let _ = tree;
                native.retained += used as u64;
                true
            }
            Err(_) => false,
        };
        let p = peak() - base;
        drop(arena);
        if !ok {
            continue;
        }
        native.peak += p;
        native.max_peak = native.max_peak.max(p);
        native.calls += calls() - c0;
        source += contents.len() as u64;
        counted += 1;

        // The whole native pipeline: syntax tree, checks, analysis tree.
        // Only the analysis tree lands in this arena (the syntax tree is
        // built elsewhere and dropped), so this counts it plus the comment
        // table on the heap.
        let (base, c0) = (live(), calls());
        reset_peak();
        let mut arena = bumpalo::Bump::new();
        let r = hakana_parser::parse_file(&arena, &contents, path);
        std::hint::black_box(&r);
        let heap_before_drop = live();
        drop(r);
        let comments = heap_before_drop - live();
        let used: usize = arena.iter_allocated_chunks().map(|c| c.len()).sum();
        native_aast.retained += used as u64 + comments;
        let p = peak() - base;
        native_aast.peak += p;
        native_aast.max_peak = native_aast.max_peak.max(p);
        drop(arena);
        native_aast.calls += calls() - c0;

        // Native parse + conversion to HHVM's AST.
        let (base, c0) = (live(), calls());
        reset_peak();
        let r = hakana_parser_bench::native(path, &contents);
        bridge.retained += live() - base;
        let p = peak() - base;
        bridge.peak += p;
        bridge.max_peak = bridge.max_peak.max(p);
        drop(r);
        bridge.calls += calls() - c0;

        // HHVM.
        let (base, c0) = (live(), calls());
        reset_peak();
        let r = hakana_parser_bench::hhvm_unmodified(path, &contents);
        hhvm.retained += live() - base;
        let p = peak() - base;
        hhvm.peak += p;
        hhvm.max_peak = hhvm.max_peak.max(p);
        drop(r);
        hhvm.calls += calls() - c0;
    }
    let mb = |b: u64| b as f64 / 1e6;
    println!("files measured: {}  source: {:.1} MB", counted, mb(source));
    println!(
        "{:<28} {:>14} {:>10} {:>16} {:>14} {:>14}",
        "", "tree (MB)", "x source", "build peak (MB)", "worst file", "malloc calls"
    );
    for (name, a) in [
        ("HHVM parser (oxidized AST)", &hhvm),
        ("native parser + bridge", &bridge),
        ("native analysis tree + checks", &native_aast),
        ("native syntax tree only", &native),
    ] {
        println!(
            "{:<28} {:>14.1} {:>10.2} {:>16.1} {:>11.1} MB {:>14}",
            name,
            mb(a.retained),
            a.retained as f64 / source as f64,
            mb(a.peak),
            mb(a.max_peak),
            a.calls
        );
    }
    println!("(tree = bytes kept alive by the result; build peak = sum over files of the");
    println!(" high-water mark while parsing one file; the arena row counts used arena bytes.)");
}

/// Prints what both parsers report for the file at `path`, and the syntax
/// tree HHVM's parser recovers for it.
fn dump(path: &str) {
    use hakana_parser_bench::{Native, ReportedError};
    let contents = std::fs::read_to_string(path).unwrap();
    let print = |title: &str, errors: &[ReportedError]| {
        println!("{title}:");
        for e in errors {
            println!("  [{}-{}] {}", e.start, e.end, e.message);
        }
    };
    match hakana_parser_bench::native(path, &contents) {
        Native::Parsed { errors, .. } => print("native", &errors),
        Native::Failed(errors) => print("native (syntax error)", &errors),
    }
    match hakana_parser_bench::hhvm(path, &contents) {
        Ok((_, _, errors)) => print("hhvm", &errors),
        Err(message) => println!("hhvm: {message}"),
    }
    let options = cst_options();
    let (tree, errors) = hakana_parser_bench::hhvm_cst_with(path, &contents, options);
    print("hhvm's parser", &errors);
    match hakana_parser::parse_cst_named(&contents, path, options) {
        Ok(native) => {
            let errors: Vec<ReportedError> = native
                .errors
                .iter()
                .map(|e| ReportedError {
                    start: e.start as usize,
                    end: e.end as usize,
                    message: e.message.to_string(),
                })
                .collect();
            print("native parser", &errors);
            match hakana_parser::cst::first_difference(&native.root, &tree) {
                Some(difference) => {
                    println!("trees differ: {difference}");
                    if std::env::var_os("CST_DUMP").is_some() {
                        println!("native tree:\n{}", native.root.dump(&contents));
                    }
                }
                None => println!("trees agree"),
            }
        }
        Err(e) => println!("native parser gave up: {e}"),
    }
    println!("hhvm's tree:\n{}", tree.dump(&contents));
}

/// Compares the errors both parsers report for one file. With
/// `SHOW_FAILURES=1` prints one line per disagreement:
/// `MISSED` (HHVM reports it, native does not), `FALSE` (native reports
/// something for a file HHVM accepts), `DIFFER`, or `ORDER` (the same
/// errors in another order). Each line shows the
/// first error of a list; `SHOW_FAILURES=all` shows the whole lists.
fn errors(path: &str, contents: &str) -> bool {
    use hakana_parser_bench::{ErrorOutcome, ReportedError, compare_errors};
    let show = std::env::var_os("SHOW_FAILURES");
    let all = show.as_deref().is_some_and(|v| v == "all");
    let show = show.is_some();
    let fmt = |errors: &[ReportedError]| {
        let shown = if all { errors } else { &errors[..1] };
        shown
            .iter()
            .map(|e| format!("[{}-{}] {}", e.start, e.end, e.message.replace('\n', " ")))
            .collect::<Vec<_>>()
            .join(" ")
    };
    match compare_errors(path, contents) {
        ErrorOutcome::BothClean | ErrorOutcome::SameErrors => true,
        ErrorOutcome::Missed(hhvm) => {
            if show {
                eprintln!("MISSED {}: {}", path, fmt(&hhvm));
            }
            false
        }
        ErrorOutcome::FalsePositive(native) => {
            if show {
                eprintln!("FALSE {}: {}", path, fmt(&native));
            }
            false
        }
        ErrorOutcome::DifferentOrder { native, hhvm } => {
            if show {
                // The first pair that differs, or everything.
                let at = native
                    .iter()
                    .zip(&hhvm)
                    .position(|(n, h)| n != h)
                    .unwrap_or(native.len().min(hhvm.len()));
                eprintln!(
                    "ORDER {}: native {} | hhvm {}",
                    path,
                    fmt(&native[if all { 0 } else { at.min(native.len() - 1) }..]),
                    fmt(&hhvm[if all { 0 } else { at.min(hhvm.len() - 1) }..])
                );
            }
            false
        }
        ErrorOutcome::DifferentErrors { native, hhvm } => {
            if show {
                eprintln!(
                    "DIFFER {}: native {} | hhvm {}",
                    path,
                    fmt(&native),
                    fmt(&hhvm)
                );
            }
            false
        }
    }
}

/// Compares what the native readonly analysis and HHVM's report for the
/// native tree of one file, or of its `variant`th variant with `readonly`
/// injected (`readonly-injected` mode). They have to agree on every tree,
/// so a file fails if they differ; with `SHOW_FAILURES=1` that prints a
/// `READONLY` line saying how. `READONLY_DUMP_DIR=<dir>` saves the variants
/// that differ there.
fn readonly(path: &str, contents: &str, variant: Option<usize>) -> bool {
    let Some(outcome) = hakana_parser_bench::compare_readonly(path, contents) else {
        // No native tree: nothing to analyse. A variant that no longer
        // parses is no loss, and files that do not parse are what the
        // `diff` and `errors` modes are about.
        return true;
    };
    READONLY_TREES.fetch_add(1, Ordering::Relaxed);
    if outcome.uses_readonly {
        READONLY_USERS.fetch_add(1, Ordering::Relaxed);
    }
    if outcome.errors > 0 {
        READONLY_ERROR_TREES.fetch_add(1, Ordering::Relaxed);
        READONLY_ERRORS.fetch_add(outcome.errors as u64, Ordering::Relaxed);
    }
    let Some(difference) = outcome.difference else {
        return true;
    };
    READONLY_DIFFERENCES.fetch_add(1, Ordering::Relaxed);
    if std::env::var_os("SHOW_FAILURES").is_some() {
        match variant {
            Some(n) => eprintln!("READONLY {path} (variant {n}): {difference}"),
            None => eprintln!("READONLY {path}: {difference}"),
        }
    }
    if let (Some(n), Ok(dir)) = (variant, std::env::var("READONLY_DUMP_DIR")) {
        let _ = std::fs::create_dir_all(&dir);
        let name = path.replace('/', "_");
        let _ = std::fs::write(format!("{dir}/{name}.{n}.hack"), contents);
    }
    false
}
