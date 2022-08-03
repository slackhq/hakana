use std::{
    collections::{BTreeMap, HashMap, HashSet},
    rc::Rc,
};

use hakana_algebra::Clause;
use hakana_reflection_info::{assertion::Assertion, t_union::TUnion};

use super::control_action::ControlAction;

#[derive(Clone, Debug)]
pub struct IfScope {
    pub new_vars: Option<BTreeMap<String, TUnion>>,

    pub new_vars_possibly_in_scope: HashSet<String>,

    pub redefined_vars: Option<HashMap<String, TUnion>>,

    pub assigned_var_ids: Option<HashMap<String, usize>>,

    pub possibly_assigned_var_ids: HashSet<String>,

    pub possibly_redefined_vars: HashMap<String, TUnion>,

    pub updated_vars: HashSet<String>,

    pub negated_types: BTreeMap<String, Vec<Vec<Assertion>>>,

    pub if_cond_changed_var_ids: HashSet<String>,

    pub negated_clauses: Vec<Clause>,

    /**
     * These are the set of clauses that could be applied after the `if`
     * statement, if the `if` statement contains branches with leaving statements,
     * and the else leaves too
     */
    pub reasonable_clauses: Vec<Rc<Clause>>,

    pub final_actions: HashSet<ControlAction>,

    pub if_actions: HashSet<ControlAction>,
}

impl<'a> IfScope {
    pub fn new() -> Self {
        Self {
            new_vars: None,
            new_vars_possibly_in_scope: HashSet::new(),
            redefined_vars: None,
            assigned_var_ids: None,
            possibly_assigned_var_ids: HashSet::new(),
            possibly_redefined_vars: HashMap::new(),
            updated_vars: HashSet::new(),
            negated_types: BTreeMap::new(),
            if_cond_changed_var_ids: HashSet::new(),
            negated_clauses: Vec::new(),
            reasonable_clauses: Vec::new(),
            final_actions: HashSet::new(),
            if_actions: HashSet::new(),
        }
    }
}
