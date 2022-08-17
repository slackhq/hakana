use std::{collections::BTreeMap, sync::Arc};

use oxidized::{ast_defs::Pos, prim_defs::Comment};

#[derive(Clone)]
pub struct FileSource {
    pub file_path: Arc<String>,
    pub hh_fixmes: BTreeMap<isize, BTreeMap<isize, Pos>>,
    pub comments: Vec<(Pos, Comment)>,
}
