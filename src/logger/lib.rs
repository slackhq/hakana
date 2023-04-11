use tower_lsp::lsp_types::MessageType;

pub enum Logger {
    DevNull,
    CommandLine(Verbosity),
    LanguageServer(tower_lsp::Client, Verbosity),
}

impl Logger {
    pub async fn log(&self, message: &str) {
        match self {
            Logger::DevNull => {}
            Logger::CommandLine(_) => {
                println!("{}", message);
            }
            Logger::LanguageServer(client, _) => {
                client.log_message(MessageType::INFO, message).await;
            }
        }
    }

    pub async fn log_debug(&self, message: &str) -> () {
        match self {
            Logger::DevNull => {}
            Logger::CommandLine(verbosity) => {
                if matches!(verbosity, Verbosity::Debugging | Verbosity::DebuggingByLine) {
                    println!("{}", message);
                }
            }
            Logger::LanguageServer(client, verbosity) => {
                if matches!(verbosity, Verbosity::Debugging | Verbosity::DebuggingByLine) {
                    client.log_message(MessageType::INFO, message).await;
                }
            }
        }
    }

    pub fn can_log_timing(&self) -> bool {
        match self {
            Logger::DevNull => false,
            Logger::CommandLine(verbosity) | Logger::LanguageServer(_, verbosity) => {
                matches!(verbosity, Verbosity::Debugging | Verbosity::Timing)
            }
        }
    }

    pub fn get_verbosity(&self) -> Verbosity {
        match self {
            Logger::DevNull => Verbosity::Simple,
            Logger::CommandLine(verbosity) | Logger::LanguageServer(_, verbosity) => *verbosity,
        }
    }

    pub fn show_progress(&self) -> bool {
        match self {
            Logger::CommandLine(Verbosity::Simple) => true,
            _ => false,
        }
    }
}

#[derive(Copy, Clone)]
pub enum Verbosity {
    Simple,
    Timing,
    Debugging,
    DebuggingByLine,
}
