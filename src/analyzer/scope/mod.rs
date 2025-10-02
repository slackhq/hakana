use std::cell::RefCell;
use std::{collections::BTreeMap, rc::Rc};

use control_action::ControlAction;
use hakana_algebra::clause::ClauseKey;
use hakana_algebra::Clause;
use hakana_code_info::function_context::FunctionContext;
use hakana_code_info::var_name::VarName;
use hakana_code_info::EFFECT_PURE;
use hakana_code_info::{assertion::Assertion, t_union::TUnion};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    function_analysis_data::FunctionAnalysisData, reconciler::assertion_reconciler,
    statements_analyzer::StatementsAnalyzer, stmt::control_analyzer::BreakContext,
};

pub mod control_action;
pub mod if_scope;
pub mod loop_scope;
pub mod switch_scope;

#[derive(Clone, Debug)]
pub struct FinallyScope {
    pub locals: BTreeMap<VarName, Rc<TUnion>>,
}

#[derive(Clone, Debug)]
pub struct CaseScope {
    pub break_vars: Option<FxHashMap<VarName, TUnion>>,
}

impl Default for CaseScope {
    fn default() -> Self {
        Self::new()
    }
}

impl CaseScope {
    pub fn new() -> Self {
        Self { break_vars: None }
    }
}

#[derive(Clone, Debug)]
pub struct BlockContext {
    /**
     * Stores the local variables for the current function being analyzed and
     * also any properties that have assertions e.g. $foo and $foo->bar would
     * both get entries if the function contained an assertion about $foo->bar.
     */
    pub locals: BTreeMap<VarName, Rc<TUnion>>,

    /**
     * A list of variables that have been referenced
     */
    pub cond_referenced_var_ids: FxHashSet<VarName>,

    /**
     * A list of vars that have been assigned to
     */
    pub assigned_var_ids: FxHashMap<VarName, usize>,

    /**
     * A list of vars that have been may have been assigned to
     */
    pub possibly_assigned_var_ids: FxHashSet<VarName>,

    /**
     * Whether or not we're inside the conditional of an if/where etc.
     *
     * This changes whether or not the context is cloned
     */
    pub inside_conditional: bool,

    /**
     * Whether or not we're inside an isset call
     *
     * Inside issets Hakana is more lenient about certain things
     */
    pub inside_isset: bool,

    /**
     * Whether or not we're inside an unset call, where
     * we don't care about possibly undefined variables
     */
    pub inside_unset: bool,

    /**
     * Whether or not we're inside an class_exists call, where
     * we don't care about possibly undefined classes
     */
    pub inside_class_exists: bool,

    /**
     * Whether or not we're inside a function/method call
     */
    pub inside_general_use: bool,

    /**
     * Whether or not we're inside an await
     */
    pub inside_await: bool,

    /**
     * Whether or not we're inside a return expression
     */
    pub inside_return: bool,

    /**
     * Whether or not we're inside a throw
     */
    pub inside_throw: bool,

    /**
     * Whether or not we're inside an assignment
     */
    pub inside_assignment: bool,

    /// Whether or not we're inside an assignment operator (i.e. +=, -=, *=, /=, %=, etc)
    pub inside_assignment_op: bool,

    pub inside_awaitall: bool,

    /**
     * A list of clauses in Conjunctive Normal Form
     */
    pub clauses: Vec<Rc<Clause>>,

    /**
     * A list of hashed clauses that have already been factored in
     */
    pub reconciled_expression_clauses: Vec<Rc<Clause>>,

    /**
     * If we've branched from the main scope, a byte offset for where that branch happened
     */
    pub branch_point: Option<usize>,

    /**
     * What does break mean in this context?
     *
     * 'loop' means we're breaking out of a loop,
     * 'switch' means we're breaking out of a switch
     */
    pub break_types: Vec<BreakContext>,

    pub inside_loop: bool,

    pub inside_loop_exprs: bool,

    /// The current case scope, if we're in a switch
    pub case_scope: Option<CaseScope>,

    /// The current finally scope, if we're in a try
    pub finally_scope: Option<Rc<RefCell<FinallyScope>>>,

    /// Details of the function that's being analyzed
    pub function_context: FunctionContext,

    /// The id of the closure that's being analyzed, if any.
    /// This may be different from the overall function context.
    pub calling_closure_id: Option<u32>,

    pub inside_negation: bool,

    pub has_returned: bool,

    pub parent_conflicting_clause_vars: FxHashSet<VarName>,

    pub allow_taints: bool,

    pub inside_async: bool,

    pub loop_bounds: (u32, u32),

    pub for_loop_init_bounds: (u32, u32),

    /* Effects for pipe var, if applicable */
    pub pipe_var_effects: u8,

    pub if_body_context: Option<Rc<RefCell<Self>>>,

    pub control_actions: FxHashSet<ControlAction>,
}

impl BlockContext {
    pub fn new(function_context: FunctionContext) -> Self {
        Self {
            locals: BTreeMap::new(),
            cond_referenced_var_ids: FxHashSet::default(),
            assigned_var_ids: FxHashMap::default(),
            possibly_assigned_var_ids: FxHashSet::default(),

            inside_conditional: false,
            inside_isset: false,
            inside_unset: false,
            inside_class_exists: false,
            inside_general_use: false,
            inside_return: false,
            inside_throw: false,
            inside_assignment: false,
            inside_assignment_op: false,
            inside_awaitall: false,
            inside_loop_exprs: false,
            inside_await: false,

            inside_negation: false,
            has_returned: false,
            clauses: Vec::new(),
            reconciled_expression_clauses: Vec::new(),

            branch_point: None,
            break_types: Vec::new(),
            inside_loop: false,
            case_scope: None,
            finally_scope: None,
            function_context,
            calling_closure_id: None,
            parent_conflicting_clause_vars: FxHashSet::default(),
            allow_taints: true,
            inside_async: false,
            loop_bounds: (0, 0),
            for_loop_init_bounds: (0, 0),

            pipe_var_effects: EFFECT_PURE,

            if_body_context: None,
            control_actions: FxHashSet::default(),
        }
    }

    pub fn get_redefined_locals(
        &self,
        new_locals: &BTreeMap<VarName, Rc<TUnion>>,
        include_new_vars: bool, // default false
        removed_vars: &mut FxHashSet<VarName>,
    ) -> FxHashMap<VarName, TUnion> {
        let mut redefined_vars = FxHashMap::default();

        let mut var_ids = self.locals.keys().collect::<Vec<_>>();
        var_ids.extend(new_locals.keys());

        for var_id in var_ids {
            if let Some(this_type) = self.locals.get(var_id) {
                if let Some(new_type) = new_locals.get(var_id) {
                    if new_type != this_type {
                        redefined_vars.insert(var_id.clone(), (**this_type).clone());
                    }
                } else if include_new_vars {
                    redefined_vars.insert(var_id.clone(), (**this_type).clone());
                }
            } else {
                removed_vars.insert(var_id.clone());
            }
        }

        redefined_vars
    }

    pub fn get_new_or_updated_locals(
        original_context: &Self,
        new_context: &Self,
    ) -> FxHashSet<VarName> {
        let mut redefined_var_ids = FxHashSet::default();

        for (var_id, new_type) in &new_context.locals {
            if let Some(original_type) = original_context.locals.get(var_id) {
                if original_context.assigned_var_ids.get(var_id).unwrap_or(&0)
                    != new_context.assigned_var_ids.get(var_id).unwrap_or(&0)
                    || original_type != new_type
                {
                    redefined_var_ids.insert(var_id.clone());
                }
            } else {
                redefined_var_ids.insert(var_id.clone());
            }
        }

        redefined_var_ids
    }

    pub fn remove_reconciled_clause_refs(
        clauses: &Vec<Rc<Clause>>,
        changed_var_ids: &FxHashSet<VarName>,
    ) -> (Vec<Rc<Clause>>, Vec<Rc<Clause>>) {
        let mut included_clauses = Vec::new();
        let mut rejected_clauses = Vec::new();

        'outer: for c in clauses {
            if c.wedge {
                included_clauses.push(c.clone());
                continue;
            }

            for key in c.possibilities.keys() {
                for changed_var_id in changed_var_ids {
                    if let ClauseKey::Name(var_name) = key {
                        if changed_var_id == var_name || var_has_root(&var_name, changed_var_id) {
                            rejected_clauses.push(c.clone());
                            continue 'outer;
                        }
                    }
                }
            }

            included_clauses.push(c.clone());
        }

        (included_clauses, rejected_clauses)
    }

    pub fn remove_reconciled_clauses(
        clauses: &Vec<Clause>,
        changed_var_ids: &FxHashSet<VarName>,
    ) -> (Vec<Clause>, Vec<Clause>) {
        let mut included_clauses = Vec::new();
        let mut rejected_clauses = Vec::new();

        'outer: for c in clauses {
            if c.wedge {
                included_clauses.push(c.clone());
                continue;
            }

            for key in c.possibilities.keys() {
                if let ClauseKey::Name(var_name) = key {
                    if changed_var_ids.contains(var_name) {
                        rejected_clauses.push(c.clone());
                        continue 'outer;
                    }
                }
            }

            included_clauses.push(c.clone());
        }

        (included_clauses, rejected_clauses)
    }

    pub(crate) fn filter_clauses(
        remove_var_id: &str,
        clauses: Vec<Rc<Clause>>,
        new_type: Option<&TUnion>,
        statements_analyzer: Option<&StatementsAnalyzer>,
        analysis_data: &mut FunctionAnalysisData,
    ) -> Vec<Rc<Clause>> {
        let mut clauses_to_keep = Vec::new();

        let mut other_clauses = Vec::new();

        'outer: for clause in clauses {
            for var_id in clause.possibilities.keys() {
                if let ClauseKey::Name(var_name) = var_id {
                    if var_has_root(var_name, remove_var_id) {
                        break 'outer;
                    }
                }
            }

            let keep_clause = should_keep_clause(&clause, remove_var_id, new_type);

            if keep_clause {
                clauses_to_keep.push(clause.clone())
            } else {
                other_clauses.push(clause);
            }
        }

        if let Some(statements_analyzer) = statements_analyzer {
            if let Some(new_type) = new_type {
                if !new_type.is_mixed() {
                    let clause_key = ClauseKey::Name(VarName::new(remove_var_id.to_string()));

                    for clause in other_clauses {
                        let mut type_changed = false;

                        // if the clause contains any possibilities that would be altered
                        // by the new type
                        for (_, assertion) in clause.possibilities.get(&clause_key).unwrap() {
                            // if we're negating a type, we generally don't need the clause anymore
                            if assertion.has_negation() {
                                type_changed = true;
                                break;
                            }

                            let result_type = assertion_reconciler::reconcile(
                                assertion,
                                Some(&new_type.clone()),
                                false,
                                None,
                                statements_analyzer,
                                analysis_data,
                                false,
                                None,
                                &None,
                                false,
                                false,
                                &FxHashMap::default(),
                            );

                            if result_type != *new_type {
                                type_changed = true;
                                break;
                            }
                        }

                        if !type_changed {
                            clauses_to_keep.push(clause.clone());
                        }
                    }
                }
            }
        }

        clauses_to_keep
    }

    pub(crate) fn remove_var_from_conflicting_clauses(
        &mut self,
        remove_var_id: &str,
        new_type: Option<&TUnion>,
        statements_analyzer: Option<&StatementsAnalyzer>,
        analysis_data: &mut FunctionAnalysisData,
    ) {
        self.clauses = BlockContext::filter_clauses(
            remove_var_id,
            self.clauses.clone(),
            new_type,
            statements_analyzer,
            analysis_data,
        );
        self.parent_conflicting_clause_vars
            .insert(VarName::new(remove_var_id.to_string()));
    }

    pub(crate) fn remove_descendants(
        &mut self,
        remove_var_id: &str,
        existing_type: &TUnion,
        new_type: Option<&TUnion>,
        statements_analyzer: Option<&StatementsAnalyzer>,
        analysis_data: &mut FunctionAnalysisData,
    ) {
        self.remove_var_from_conflicting_clauses(
            remove_var_id,
            if existing_type.is_mixed() {
                None
            } else if let Some(new_type) = new_type {
                Some(new_type)
            } else {
                None
            },
            statements_analyzer,
            analysis_data,
        );

        let keys = self.locals.keys().cloned().collect::<Vec<_>>();

        for var_id in keys {
            if var_has_root(&var_id, remove_var_id) {
                self.locals.remove(&var_id);
            }
        }
    }

    pub(crate) fn remove_mutable_object_vars(&mut self) {
        let mut removed_var_ids = vec![];

        self.locals.retain(|var_id, _| {
            let retain = !var_id.contains("->") && !var_id.contains("::");
            if !retain {
                removed_var_ids.push(var_id.clone());
            }
            retain
        });

        if removed_var_ids.is_empty() {
            return;
        }

        self.clauses.retain(|clause| {
            let mut retain_clause = true;

            for var_id in clause.possibilities.keys() {
                if let ClauseKey::Name(var_id) = var_id {
                    if var_id.contains("->") || var_id.contains("::") {
                        retain_clause = false;
                    }
                }
            }

            retain_clause
        });
    }

    pub(crate) fn has_variable(&mut self, var_name: &str) -> bool {
        self.cond_referenced_var_ids
            .insert(VarName::new(var_name.to_string()));

        self.locals.contains_key(var_name)
    }
}

fn should_keep_clause(clause: &Rc<Clause>, remove_var_id: &str, new_type: Option<&TUnion>) -> bool {
    clause
        .possibilities
        .get(&ClauseKey::Name(VarName::new(remove_var_id.to_string())))
        .map_or(true, |possibilities| {
            if possibilities.len() == 1 {
                let assertion = possibilities.values().next().unwrap();

                if let Assertion::IsType(assertion_type) = assertion {
                    if let Some(new_type) = new_type {
                        if new_type.is_single() {
                            return new_type.get_single() == assertion_type;
                        }
                    }
                }
            }

            false
        })
}

#[inline]
pub fn var_has_root(var_id: &str, root_var_id: &str) -> bool {
    if let Some(pos) = var_id.find(root_var_id) {
        if var_id == root_var_id {
            return false;
        }
        let bytes = var_id.as_bytes();
        if pos > 0 && (bytes[pos - 1] as char) == ':' {
            return false;
        }
        let i = root_var_id.len() + pos;
        return matches!(bytes[i] as char, '[' | '-' | ']');
    }

    false
}
