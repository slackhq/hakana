use std::{collections::BTreeMap, rc::Rc};

use hakana_algebra::Clause;
use hakana_reflection_info::{assertion::Assertion, t_union::TUnion};
use rustc_hash::{FxHashMap, FxHashSet};

use super::control_action::ControlAction;

#[derive(Clone, Debug)]
pub struct IfScope {
    pub new_vars: Option<BTreeMap<String, TUnion>>,

    pub new_vars_possibly_in_scope: FxHashSet<String>,

    pub redefined_vars: Option<FxHashMap<String, TUnion>>,

    pub removed_var_ids: FxHashSet<String>,

    pub assigned_var_ids: Option<FxHashMap<String, usize>>,

    pub possibly_assigned_var_ids: FxHashSet<String>,

    pub possibly_redefined_vars: FxHashMap<String, TUnion>,

    pub updated_vars: FxHashSet<String>,

    pub negated_types: BTreeMap<String, Vec<Vec<Assertion>>>,

    pub if_cond_changed_var_ids: FxHashSet<String>,

    pub negated_clauses: Vec<Clause>,

    /**
     * These are the set of clauses that could be applied after the `if`
     * statement, if the `if` statement contains branches with leaving statements,
     * and the else leaves too
     */
    pub reasonable_clauses: Vec<Rc<Clause>>,

    pub final_actions: FxHashSet<ControlAction>,

    pub if_actions: FxHashSet<ControlAction>,
}

impl<'a> IfScope {
    pub fn new() -> Self {
        Self {
            new_vars: None,
            new_vars_possibly_in_scope: FxHashSet::default(),
            redefined_vars: None,
            assigned_var_ids: None,
            possibly_assigned_var_ids: FxHashSet::default(),
            possibly_redefined_vars: FxHashMap::default(),
            updated_vars: FxHashSet::default(),
            negated_types: BTreeMap::new(),
            if_cond_changed_var_ids: FxHashSet::default(),
            negated_clauses: Vec::new(),
            reasonable_clauses: Vec::new(),
            final_actions: FxHashSet::default(),
            if_actions: FxHashSet::default(),
            removed_var_ids: FxHashSet::default(),
        }
    }
}
