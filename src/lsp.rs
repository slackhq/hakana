use hakana_analyzer::config;
use hakana_workhorse::scanner::scan_files;
use mimalloc::MiMalloc;

use std::{env, sync::Arc};

use hakana_language_server::{get_config, Backend};
use tower_lsp::{LspService, Server};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let cwd = (env::current_dir()).unwrap().to_str().unwrap().to_string();
    let config = Arc::new(get_config(vec![], &cwd));

    let scan_result = scan_files(
        &vec![config.root_dir.clone()],
        None,
        &config,
        8,
        config::Verbosity::Quiet,
        "",
        None,
    )?;

    let (service, socket) = LspService::new(|client| Backend {
        client,
        analysis_config: config,
        scan_result: scan_result.into(),
    });
    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}
