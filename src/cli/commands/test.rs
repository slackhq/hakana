use clap::Args;
use rand::Rng;

use crate::test_runners::test_runner::TestRunner;

#[derive(Args)]
#[command(arg_required_else_help = true)]
pub struct TestArgs {
    /// Whether to use cache
    #[arg(long)]
    pub no_cache: bool,

    /// Whether to reuse codebase between tests
    #[arg(long)]
    pub reuse_codebase: bool,

    /// Whether to randomise test order
    #[arg(long)]
    pub randomize: bool,

    /// Seed for random test execution
    #[arg(long, value_name = "COUNT")]
    pub seed: Option<u64>,

    /// Whether to show debug output
    #[arg(long)]
    pub debug: bool,

    /// How many times to repeat the test (useful for profiling)
    #[arg(long, value_name = "COUNT")]
    pub repeat: Option<u16>,

    /// Rewrite expectation files for tests whose output changed
    #[arg(long)]
    pub update_snapshots: bool,

    /// The test to run
    #[arg(value_name = "TEST")]
    pub test: String,
}

pub fn handle(
    args: &TestArgs,
    test_runner: &TestRunner,
    show_progress: bool,
    had_error: &mut bool,
    header: &str,
) {
    let repeat = args.repeat.unwrap_or(0);

    let random_seed = args
        .randomize
        .then(|| args.seed.unwrap_or_else(|| rand::thread_rng().r#gen()));

    test_runner.run_test(
        args.test.clone(),
        show_progress,
        !args.no_cache,
        args.reuse_codebase,
        had_error,
        header,
        repeat,
        random_seed,
        args.update_snapshots,
    );
}
