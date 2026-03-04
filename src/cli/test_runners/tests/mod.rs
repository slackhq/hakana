mod code_transform;
mod diff;
mod executable_code_finder;
mod goto_definition;
mod linter;
mod migration_candidates;
mod references;
mod skipped;
mod standard_analysis;

pub use code_transform::CodeTransformTest;
pub use diff::DiffTest;
pub use executable_code_finder::ExecutableCodeFinderTest;
pub use goto_definition::GotoDefinitionTest;
pub use linter::LinterTest;
pub use migration_candidates::MigrationCandidatesTest;
pub use references::ReferencesTest;
pub use skipped::SkippedTest;
pub use standard_analysis::StandardAnalysisTest;
