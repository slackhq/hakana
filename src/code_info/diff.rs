use rustc_hash::FxHashMap;

use crate::{code_location::FilePath, StrId};

#[derive(Default, Debug)]
pub struct CodebaseDiff {
    pub keep: Vec<(StrId, StrId)>,
    pub keep_signature: Vec<(StrId, StrId)>,
    pub add_or_delete: Vec<(StrId, StrId)>,
    pub diff_map: FxHashMap<FilePath, Vec<(usize, usize, isize, isize)>>,
    pub deletion_ranges_map: FxHashMap<FilePath, Vec<(usize, usize)>>,
}

impl CodebaseDiff {
    pub fn extend(&mut self, other: Self) {
        self.keep.extend(other.keep);
        self.keep_signature.extend(other.keep_signature);
        self.add_or_delete.extend(other.add_or_delete);
        self.diff_map.extend(other.diff_map);
        self.deletion_ranges_map.extend(other.deletion_ranges_map);
    }
}
