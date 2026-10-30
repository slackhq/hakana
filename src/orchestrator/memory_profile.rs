//! Opt-in snapshots of retained objects around codebase population.
//!
//! Build with `--features hakana-cli/memory-profile` and set
//! `HAKANA_MEMORY_PROFILE` to an output directory. No object traversal runs in
//! ordinary builds, or in profiling builds when that variable is absent.
use std::{
    collections::BTreeMap,
    fs, io,
    mem::size_of_val,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

use allocative::{Allocative, FlameGraphBuilder, Key, Visitor};
use hakana_code_info::{
    code_location::FilePath, codebase_info::CodebaseInfo, symbol_references::SymbolReferences,
};
use hakana_str::{Interner, StrId, memory_profile as containers};
use rustc_hash::FxHashMap;
use serde::Serialize;

use crate::file::VirtualFileSystem;

static NEXT_SNAPSHOT: AtomicUsize = AtomicUsize::new(0);

pub(crate) struct MemoryProfile {
    directory: PathBuf,
    prefix: String,
}

pub(crate) struct Roots<'a> {
    pub codebase: &'a CodebaseInfo,
    pub interner: &'a Interner,
    pub resolved_names: &'a FxHashMap<FilePath, FxHashMap<u32, StrId>>,
    pub file_system: &'a VirtualFileSystem,
    pub symbol_references: &'a SymbolReferences,
}

impl Allocative for Roots<'_> {
    fn visit<'a, 'b: 'a>(&self, visitor: &'a mut Visitor<'b>) {
        // These are borrowed views of the real roots. Visit the objects, not
        // the references, and use one builder to deduplicate shared allocations.
        visitor.visit_field(Key::new("codebase"), self.codebase);
        visitor.visit_field(Key::new("interner"), self.interner);
        visitor.visit_field(Key::new("symbol_references"), self.symbol_references);
        visitor.visit_field_with(
            Key::new("resolved_names"),
            size_of_val(self.resolved_names),
            |visitor| containers::hash_map_of_maps(self.resolved_names, visitor),
        );
        visitor.visit_field_with(
            Key::new("file_hashes_and_times"),
            size_of_val(&self.file_system.file_hashes_and_times),
            |visitor| containers::hash_map(&self.file_system.file_hashes_and_times, visitor),
        );
    }
}

#[derive(Serialize)]
struct Snapshot {
    stage: String,
    estimated_retained_bytes: usize,
    process_rss_bytes_before_traversal: Option<u64>,
    traversal_seconds: f64,
    roots: BTreeMap<String, u64>,
    /// Disjoint contributions; sum these to recover estimated_retained_bytes.
    allocations: Vec<Allocation>,
    counts: BTreeMap<&'static str, usize>,
    inline_type_bytes: BTreeMap<&'static str, usize>,
    accounting_warnings: String,
    methodology: &'static str,
}

#[derive(Serialize)]
struct Allocation {
    path: String,
    self_bytes: u64,
}

impl MemoryProfile {
    pub(crate) fn from_env() -> io::Result<Option<Self>> {
        let Some(directory) = std::env::var_os("HAKANA_MEMORY_PROFILE") else {
            return Ok(None);
        };
        let directory = PathBuf::from(directory);
        fs::create_dir_all(&directory)?;
        Ok(Some(Self {
            directory,
            prefix: format!(
                "{}-{}",
                std::process::id(),
                NEXT_SNAPSHOT.fetch_add(1, Ordering::Relaxed)
            ),
        }))
    }

    pub(crate) fn write(&self, stage: &str, roots: Roots<'_>) -> io::Result<()> {
        let rss = process_rss_bytes();
        let start = Instant::now();
        let mut builder = FlameGraphBuilder::default();
        builder.visit_root(&roots);
        let output = builder.finish();
        let traversal_seconds = start.elapsed().as_secs_f64();
        let folded = output.flamegraph().write();
        let mut root_sizes = BTreeMap::<String, u64>::new();
        let allocations = folded
            .lines()
            .map(|line| {
                let (path, size) = line.rsplit_once(' ').unwrap();
                let self_bytes = size.parse().unwrap();
                *root_sizes
                    .entry(path.split(';').next().unwrap().to_owned())
                    .or_default() += self_bytes;
                Allocation {
                    path: path.to_owned(),
                    self_bytes,
                }
            })
            .collect();
        let snapshot = Snapshot {
            stage: stage.to_owned(),
            estimated_retained_bytes: output.flamegraph().total_size(),
            process_rss_bytes_before_traversal: rss,
            traversal_seconds,
            roots: root_sizes,
            allocations,
            counts: BTreeMap::from([
                ("classes", roots.codebase.classlike_infos.len()),
                (
                    "functions_and_methods",
                    roots.codebase.functionlike_infos.len(),
                ),
                ("type_definitions", roots.codebase.type_definitions.len()),
                ("files", roots.codebase.files.len()),
                ("files_with_resolved_names", roots.resolved_names.len()),
                (
                    "parameters",
                    roots
                        .codebase
                        .functionlike_infos
                        .values()
                        .map(|f| f.params.len())
                        .sum(),
                ),
                (
                    "parameter_defaults",
                    roots
                        .codebase
                        .functionlike_infos
                        .values()
                        .flat_map(|f| &f.params)
                        .filter(|p| p.default_type.is_some())
                        .count(),
                ),
                (
                    "nonempty_function_contexts",
                    roots
                        .codebase
                        .functionlike_infos
                        .values()
                        .filter(|f| {
                            f.type_resolution_context.as_ref().is_some_and(|context| {
                                !context.template_type_map.is_empty()
                                    || !context.template_supers.is_empty()
                            })
                        })
                        .count(),
                ),
                (
                    "enum_underlying_types",
                    roots
                        .codebase
                        .classlike_infos
                        .values()
                        .filter(|c| c.enum_underlying_type.is_some())
                        .count(),
                ),
                (
                    "enum_constraints",
                    roots
                        .codebase
                        .classlike_infos
                        .values()
                        .filter(|c| c.enum_as_type.is_some())
                        .count(),
                ),
            ]),
            inline_type_bytes: BTreeMap::from([
                (
                    "FunctionLikeInfo",
                    std::mem::size_of::<hakana_code_info::functionlike_info::FunctionLikeInfo>(),
                ),
                (
                    "FunctionLikeParameter",
                    std::mem::size_of::<
                        hakana_code_info::functionlike_parameter::FunctionLikeParameter,
                    >(),
                ),
                (
                    "ClassLikeInfo",
                    std::mem::size_of::<hakana_code_info::classlike_info::ClassLikeInfo>(),
                ),
            ]),
            accounting_warnings: output.warnings(),
            methodology: "Retained object graph, not allocation traffic or peak memory. Includes inline sizes, Vec/String capacities and estimated HashMap/IndexMap storage. Shared Arc/Rc payloads are counted once across all roots and reported separately at the flamegraph root. BTreeMap includes live entry payload and descendants, excluding node headers and spare slots. Excludes allocator metadata/rounding/retained free pages, thread stacks, temporary objects no longer reachable at the checkpoint, and state outside the listed roots. RSS is sampled before traversal; earlier profiling traversals may affect allocator retention.",
        };
        let base = self.directory.join(format!("{}-{stage}", self.prefix));
        fs::write(base.with_extension("folded"), folded)?;
        fs::write(
            base.with_extension("json"),
            serde_json::to_vec_pretty(&snapshot)?,
        )?;
        eprintln!(
            "Memory profile {stage}: {:.1} MiB retained (estimate), {:.2}s traversal; {}",
            snapshot.estimated_retained_bytes as f64 / (1024.0 * 1024.0),
            traversal_seconds,
            base.with_extension("json").display(),
        );
        Ok(())
    }
}

fn process_rss_bytes() -> Option<u64> {
    // ps reports current RSS in KiB on both Linux and macOS. This also avoids
    // mistaking getrusage's peak RSS for the checkpoint's current residency.
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    std::str::from_utf8(&output.stdout)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?
        .checked_mul(1024)
}
