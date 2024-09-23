use std::{error::Error, fs::File, io::BufReader, path::Path};

use rustc_hash::FxHashMap;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct JsonConfig {
    #[serde(default)]
    pub ignore_files: Vec<String>,
    #[serde(default)]
    pub ignore_issue_files: FxHashMap<String, Vec<String>>,
    #[serde(default)]
    pub banned_builtin_functions: FxHashMap<String, String>,
    #[serde(default)]
    pub security_analysis: JsonSecurityConfig,
    #[serde(default)]
    pub allowed_issues: Vec<String>,
    #[serde(default)]
    pub test_files: Vec<String>,
}

#[derive(Deserialize, Debug, Default)]
pub struct JsonSecurityConfig {
    pub ignore_files: Vec<String>,
    pub ignore_sink_files: FxHashMap<String, Vec<String>>,
    pub max_depth: u8,
}

pub(crate) fn read_from_file(path: &Path) -> Result<JsonConfig, Box<dyn Error>> {
    // Open the file in read-only mode with buffer.
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    Ok(serde_json::from_reader(reader)?)
}
