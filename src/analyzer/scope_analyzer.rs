use crate::config::Config;
use crate::file_analyzer::FileAnalyzer;

pub trait ScopeAnalyzer {
    fn get_namespace(&self) -> &Option<String>;

    fn get_file_analyzer(&self) -> &FileAnalyzer<'_>;

    fn get_config(&self) -> &Config;
}
