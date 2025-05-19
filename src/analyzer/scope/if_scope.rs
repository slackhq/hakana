use std::{collections::BTreeMap, rc::Rc};

use hakana_algebra::Clause;
use hakana_code_info::{assertion::Assertion, t_union::TUnion, var_name::VarName};
use rustc_hash::{FxHashMap, FxHashSet};

use super::control_action::ControlAction;

#[derive(Clone, Debug, Default)]
pub struct IfScope {
    pub new_vars: Option<BTreeMap<VarName, TUnion>>,

    pub new_vars_possibly_in_scope: FxHashSet<VarName>,

    pub redefined_vars: Option<FxHashMap<VarName, TUnion>>,

    pub removed_var_ids: FxHashSet<VarName>,

    pub assigned_var_ids: Option<FxHashMap<VarName, usize>>,

    pub possibly_assigned_var_ids: FxHashSet<VarName>,

    pub possibly_redefined_vars: FxHashMap<VarName, TUnion>,

    pub updated_vars: FxHashSet<VarName>,

    pub negated_types: BTreeMap<VarName, Vec<Vec<Assertion>>>,

    pub if_cond_changed_var_ids: FxHashSet<VarName>,

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
