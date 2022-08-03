use std::{collections::HashMap, error::Error, fs::File, io::BufReader, path::Path};

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct JsonConfig {
    pub ignore_files: Vec<String>,
    pub ignore_issue_files: HashMap<String, Vec<String>>,
    pub security_analysis: JsonSecurityConfig,
}

#[derive(Deserialize, Debug)]
pub struct JsonSecurityConfig {
    pub ignore_files: Vec<String>,
    pub ignore_sink_files: HashMap<String, Vec<String>>,
}

pub(crate) fn read_from_file(path: &Path) -> Result<JsonConfig, Box<dyn Error>> {
    // Open the file in read-only mode with buffer.
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    Ok(serde_json::from_reader(reader)?)
}
