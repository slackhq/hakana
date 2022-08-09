#[cfg(not(target_env = "msvc"))]
#[cfg(not(all(target_arch = "x86_64", target_os = "macos")))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[cfg(not(all(target_arch = "x86_64", target_os = "macos")))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn main() {
    hakana_cli::init(
        vec![],
        vec![],
        env!("VERGEN_GIT_SHA"),
        Box::new(hakana_cli::test_runners::core_test_runner::CoreTestRunner {}),
    );
}
