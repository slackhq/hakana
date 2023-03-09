use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    ).await;

    Ok(())
}
