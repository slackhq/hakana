use std::collections::HashSet;

pub struct FileInfo {
    pub classlikes_in_file: HashSet<String>,
    pub functions_in_file: HashSet<String>,
    pub required_classes: HashSet<String>,
    pub required_interfaces: HashSet<String>,
}

impl FileInfo {
    pub fn new() -> Self {
        Self {
            classlikes_in_file: HashSet::new(),
            functions_in_file: HashSet::new(),
            required_classes: HashSet::new(),
            required_interfaces: HashSet::new(),
        }
    }
}
