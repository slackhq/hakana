pub enum Logger {
    DevNull,
    CommandLine(Verbosity),
}

impl Logger {
    pub fn log(&self, message: &str) {
        match self {
            Logger::DevNull => {}
            Logger::CommandLine(verbosity) => {
                if !matches!(verbosity, Verbosity::Quiet) {
                    println!("{}", message);
                }
            }
        }
    }

    pub fn log_debug(&self, message: &str) -> () {
        match self {
            Logger::DevNull => {}
            Logger::CommandLine(verbosity) => {
                if matches!(verbosity, Verbosity::Debugging | Verbosity::DebuggingByLine) {
                    println!("{}", message);
                }
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
            Logger::DevNull => Verbosity::Quiet,
            Logger::CommandLine(verbosity) => *verbosity,
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
    Quiet,
    Simple,
    Timing,
    Debugging,
    DebuggingByLine,
}
