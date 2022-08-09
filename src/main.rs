#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
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
