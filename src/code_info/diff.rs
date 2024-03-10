use hakana_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::code_location::FilePath;

#[derive(Default, Debug)]
pub struct CodebaseDiff {
    pub keep: FxHashSet<(StrId, StrId)>,
    pub keep_signature: FxHashSet<(StrId, StrId)>,
    pub add_or_delete: FxHashSet<(StrId, StrId)>,
    pub diff_map: FxHashMap<FilePath, Vec<(u32, u32, isize, isize)>>,
    pub deletion_ranges_map: FxHashMap<FilePath, Vec<(u32, u32)>>,
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
