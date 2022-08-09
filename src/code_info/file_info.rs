use rustc_hash::FxHashSet;

pub struct FileInfo {
    pub classlikes_in_file: FxHashSet<String>,
    pub functions_in_file: FxHashSet<String>,
    pub required_classes: FxHashSet<String>,
    pub required_interfaces: FxHashSet<String>,
}

impl FileInfo {
    pub fn new() -> Self {
        Self {
            classlikes_in_file: FxHashSet::default(),
            functions_in_file: FxHashSet::default(),
            required_classes: FxHashSet::default(),
            required_interfaces: FxHashSet::default(),
        }
    }
}
