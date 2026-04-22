use tokio::sync::mpsc::Sender;

pub enum Logger {
    DevNull,
    CommandLine(Verbosity),
    Channel(tokio::sync::mpsc::Sender<String>),
}

impl Logger {
    fn try_send(tx: &Sender<String>, message: &str) {
        if let Err(e) = tx.blocking_send(message.to_string()) {
            eprintln!("error reporting event {}", e);
        }
    }

    pub async fn log(&self, message: &str) {
        match self {
            Logger::DevNull => {}
            Logger::CommandLine(_) => {
                println!("{}", message);
            }
            Logger::Channel(tx) => Logger::try_send(tx, message),
        }
    }

    pub fn log_sync(&self, message: &str) {
        match self {
            Logger::DevNull => {}
            Logger::CommandLine(_) => {
                println!("{}", message);
            }
            Logger::Channel(tx) => Logger::try_send(tx, message),
        }
    }

    pub async fn log_debug(&self, message: &str) {
        match self {
            Logger::DevNull | Logger::Channel(_) => {}
            Logger::CommandLine(verbosity) => {
                if matches!(verbosity, Verbosity::Debugging | Verbosity::DebuggingByLine) {
                    println!("{}", message);
                }
            }
        }
    }

    pub fn log_debug_sync(&self, message: &str) {
        if let Logger::CommandLine(verbosity) = self {
            if matches!(verbosity, Verbosity::Debugging | Verbosity::DebuggingByLine) {
                println!("{}", message);
            }
        }
    }

    pub fn can_log_timing(&self) -> bool {
        match self {
            Logger::DevNull | Logger::Channel(_) => false,
            Logger::CommandLine(verbosity) => {
                matches!(verbosity, Verbosity::Debugging | Verbosity::Timing)
            }
        }
    }

    pub fn get_verbosity(&self) -> Verbosity {
        match self {
            Logger::DevNull | Logger::Channel(_) => Verbosity::Simple,
            Logger::CommandLine(verbosity) => *verbosity,
        }
    }

    pub fn show_progress(&self) -> bool {
        matches!(self, Logger::CommandLine(Verbosity::Simple))
    }
}

#[derive(Copy, Clone)]
pub enum Verbosity {
    Simple,
    Timing,
    Debugging,
    DebuggingByLine,
}
