use hakana_algebra::Clause;
use hakana_reflection_info::t_union::TUnion;
use oxidized::aast;
use std::{
    collections::{BTreeMap, HashMap},
    rc::Rc,
};

pub(crate) struct SwitchScope {
    pub new_vars_in_scope: Option<BTreeMap<String, Rc<TUnion>>>,

    pub redefined_vars: Option<HashMap<String, Rc<TUnion>>>,

    pub possibly_redefined_vars: Option<BTreeMap<String, TUnion>>,

    pub leftover_statements: Vec<aast::Stmt<(), ()>>,

    pub leftover_case_equality_expr: Option<aast::Expr<(), ()>>,

    pub negated_clauses: Vec<Clause>,

    pub new_assigned_var_ids: HashMap<String, usize>,
}

impl<'a> SwitchScope {
    pub(crate) fn new() -> Self {
        Self {
            new_vars_in_scope: None,
            redefined_vars: None,
            possibly_redefined_vars: None,
            leftover_statements: vec![],
            leftover_case_equality_expr: None,
            negated_clauses: vec![],
            new_assigned_var_ids: HashMap::new(),
        }
    }
}
