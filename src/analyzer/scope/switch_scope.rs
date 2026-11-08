use hakana_algebra::Clause;
use hakana_code_info::{t_union::TUnion, var_name::VarName};
use hakana_parser::aast;
use rustc_hash::FxHashMap;
use std::{collections::BTreeMap, rc::Rc};

pub(crate) struct SwitchScope<'a> {
    pub new_locals: Option<BTreeMap<VarName, Rc<TUnion>>>,

    pub redefined_vars: Option<FxHashMap<VarName, Rc<TUnion>>>,

    pub possibly_redefined_vars: Option<BTreeMap<VarName, TUnion>>,

    pub leftover_statements: Vec<aast::Stmt<'a>>,

    pub leftover_case_equality_expr: Option<aast::Expr<'a>>,

    pub negated_clauses: Vec<Clause>,

    pub new_assigned_var_ids: FxHashMap<VarName, usize>,
}

impl SwitchScope<'_> {
    pub(crate) fn new() -> Self {
        Self {
            new_locals: None,
            redefined_vars: None,
            possibly_redefined_vars: None,
            leftover_statements: vec![],
            leftover_case_equality_expr: None,
            negated_clauses: vec![],
            new_assigned_var_ids: FxHashMap::default(),
        }
    }
}
