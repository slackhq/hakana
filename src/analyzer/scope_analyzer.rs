use crate::config::Config;
use crate::file_analyzer::FileAnalyzer;
use hakana_reflection_info::codebase_info::CodebaseInfo;

pub trait ScopeAnalyzer {
    fn get_namespace(&self) -> &Option<String>;

    fn get_file_analyzer(&self) -> &FileAnalyzer;

    fn get_codebase(&self) -> &CodebaseInfo;

    fn get_config(&self) -> &Config;
}
