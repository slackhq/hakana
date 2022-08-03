use std::{
    collections::{BTreeMap, HashMap, HashSet},
    rc::Rc,
};

use hakana_reflection_info::t_union::TUnion;

use super::control_action::ControlAction;

#[derive(Clone)]
pub struct LoopScope {
    pub iteration_count: usize,

    pub parent_context_vars: BTreeMap<String, Rc<TUnion>>,

    pub redefined_loop_vars: HashMap<String, TUnion>,

    pub possibly_redefined_loop_vars: HashMap<String, TUnion>,

    pub possibly_redefined_loop_parent_vars: HashMap<String, TUnion>,

    pub possibly_defined_loop_parent_vars: HashMap<String, TUnion>,

    pub protected_var_ids: HashSet<String>,

    pub final_actions: Vec<ControlAction>,
}

impl LoopScope {
    pub fn new(parent_context_vars: BTreeMap<String, Rc<TUnion>>) -> Self {
        Self {
            parent_context_vars,
            iteration_count: 0,
            redefined_loop_vars: HashMap::new(),
            possibly_redefined_loop_vars: HashMap::new(),
            possibly_redefined_loop_parent_vars: HashMap::new(),
            possibly_defined_loop_parent_vars: HashMap::new(),
            protected_var_ids: HashSet::new(),
            final_actions: Vec::new(),
        }
    }
}
