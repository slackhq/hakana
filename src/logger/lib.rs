pub enum Logger {
    DevNull,
    CommandLine(Verbosity),
}

impl Logger {
    pub async fn log(&self, message: &str) {
        match self {
            Logger::DevNull => {}
            Logger::CommandLine(_) => {
                println!("{}", message);
            }
        }
    }

    pub fn log_sync(&self, message: &str) {
        match self {
            Logger::DevNull => {}
            Logger::CommandLine(_) => {
                println!("{}", message);
            }
        }
    }

    pub async fn log_debug(&self, message: &str) {
        match self {
            Logger::DevNull => {}
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
            Logger::DevNull => false,
            Logger::CommandLine(verbosity) => {
                matches!(verbosity, Verbosity::Debugging | Verbosity::Timing)
            }
        }
    }

    pub fn get_verbosity(&self) -> Verbosity {
        match self {
            Logger::DevNull => Verbosity::Simple,
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
