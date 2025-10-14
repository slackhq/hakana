use hakana_cli::test_runners::{core_test_runner::CoreHooksProvider, test_runner::TestRunner};
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
    let build_timestamp = env!("VERGEN_BUILD_TIMESTAMP");
    let header = "\nCommit:    ".to_string()
        + &env!("VERGEN_GIT_SHA")[0..7]
        + "\nTimestamp: "
        + build_timestamp;

    hakana_cli::init(
        vec![],
        vec![],
        vec![],
        header.as_str(),
        &TestRunner(Box::new(CoreHooksProvider {})),
        vec![],
    );
}
