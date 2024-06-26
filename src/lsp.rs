use std::{env, io::Cursor};

use hakana_language_server::{get_config, Backend};
use hakana_str::Interner;
use mimalloc::MiMalloc;

use tokio::io::AsyncWriteExt;
use tower_lsp::{LspService, Server};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let mut stderr = tokio::io::stderr();

    let cwd = if let Ok(current_dir) = env::current_dir() {
        if let Some(str) = current_dir.to_str() {
            str.to_string()
        } else {
            stderr
                .write_all_buf(&mut Cursor::new(b"Passed current directory is malformed"))
                .await
                .ok();
            return;
        }
    } else {
        stderr
            .write_all_buf(&mut Cursor::new(
                b"Current working directory could not be determined",
            ))
            .await
            .ok();
        return;
    };

    let mut interner = Interner::default();

    let config = match get_config(vec![], &cwd, &mut interner) {
        Ok(config) => config,
        Err(error) => {
            stderr
                .write_all_buf(&mut Cursor::new(format!("Config error: {error}")))
                .await
                .ok();
            return;
        }
    };

    let (service, socket) = LspService::new(|client| Backend::new(client, config, interner));
    Server::new(stdin, stdout, socket).serve(service).await;
}
