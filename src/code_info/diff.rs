use rustc_hash::FxHashMap;

use crate::StrId;

#[derive(Default, Debug)]
pub struct CodebaseDiff {
    pub keep: Vec<(StrId, Option<StrId>)>,
    pub keep_signature: Vec<(StrId, Option<StrId>)>,
    pub add_or_delete: Vec<(StrId, Option<StrId>)>,
    pub diff_map: FxHashMap<StrId, Vec<(usize, usize, isize, isize)>>,
    pub deletion_ranges_map: FxHashMap<StrId, Vec<(usize, usize)>>,
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
