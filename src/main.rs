#[cfg(not(target_env = "msvc"))]
#[cfg(not(all(target_arch = "x86_64", target_os = "macos")))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[cfg(not(all(target_arch = "x86_64", target_os = "macos")))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn main() {
    let build_timestamp = env!("VERGEN_BUILD_TIMESTAMP");
    let header = "\nCommit:    ".to_string()
        + &env!("VERGEN_GIT_SHA")[0..7]
        + "\nTimestamp: "
        + &build_timestamp;

    hakana_cli::init(
        vec![],
        vec![],
        header.as_str(),
        Box::new(hakana_cli::test_runners::core_test_runner::CoreTestRunner {}),
    );
}
