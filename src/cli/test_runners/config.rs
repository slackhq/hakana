use std::{error::Error, fs::File, io::BufReader, path::Path};

use serde::Deserialize;

#[derive(Deserialize, Debug, Default)]
pub struct TestConfig {
    pub max_changes_allowed: Option<usize>,
}

pub(crate) fn read_from_file(path: &Path) -> Result<TestConfig, Box<dyn Error>> {
    // Open the file in read-only mode with buffer.
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    Ok(serde_json::from_reader(reader)?)
}
