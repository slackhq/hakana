use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
    let header = format!(
        "Hakana\n\nCommit:    {}\nTimestamp: {}",
        &env!("VERGEN_GIT_SHA")[0..7],
        env!("VERGEN_BUILD_TIMESTAMP")
    );

    hakana_cli::mcp::run(vec![], header);
}
