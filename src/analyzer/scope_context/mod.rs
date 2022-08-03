use std::{
    collections::{BTreeMap, HashMap, HashSet},
    rc::Rc,
};

use function_context::FunctionContext;
use hakana_algebra::Clause;
use hakana_reflection_info::t_union::TUnion;
use oxidized::ast_defs::Pos;
use regex::Regex;

use crate::{
    reconciler::{assertion_reconciler, reconciler},
    statements_analyzer::StatementsAnalyzer,
    stmt::control_analyzer::BreakContext,
    typed_ast::TastInfo,
};

use lazy_static::lazy_static;

pub mod control_action;
pub mod if_scope;
pub mod loop_scope;
pub mod switch_scope;

#[derive(Clone, Debug)]
pub struct FinallyScope {
    pub vars_in_scope: BTreeMap<String, Rc<TUnion>>,
}

#[derive(Clone, Debug)]
pub struct CaseScope {
    pub break_vars: Option<HashMap<String, TUnion>>,
}

impl CaseScope {
    pub fn new() -> Self {
        Self { break_vars: None }
    }
}

#[derive(Clone, Debug)]
pub struct ScopeContext {
    pub vars_in_scope: BTreeMap<String, Rc<TUnion>>,

    /**
     * A list of variables that have been referenced
     */
    pub cond_referenced_var_ids: HashSet<String>,

    /**
     * A list of vars that have been assigned to
     */
    pub assigned_var_ids: HashMap<String, usize>,

    /**
     * A list of vars that have been may have been assigned to
     */
    pub possibly_assigned_var_ids: HashSet<String>,

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
     * Whether or not we're inside a return expression
     */
    pub inside_return: bool,

    /**
     * Whether or not we're inside a throw
     */
    pub inside_throw: bool,

    /**
     * Whether or not we're inside a try
     */
    pub inside_try: bool,

    /**
     * Whether or not we're inside an assignment
     */
    pub inside_assignment: bool,

    pub include_location: Option<Pos>,

    pub check_classes: bool,

    pub check_variables: bool,

    pub check_methods: bool,

    pub check_consts: bool,

    pub check_functions: bool,

    /**
     * A list of files checked with file_exists
     */
    pub phantom_files: HashMap<String, bool>,

    /**
     * A list of clauses in Conjunctive Normal Form
     */
    pub clauses: Vec<Rc<Clause>>,

    /**
     * A list of hashed clauses that have already been factored in
     */
    pub reconciled_expression_clauses: Vec<Rc<Clause>>,

    /**
     * Variables assigned in loops that should not be overwritten
     */
    pub protected_var_ids: HashSet<String>,

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

    pub case_scope: Option<CaseScope>,

    pub finally_scope: Option<FinallyScope>,

    pub function_context: FunctionContext,

    pub inside_negation: bool,

    pub error_suppressing: bool,

    pub has_returned: bool,

    pub parent_remove_vars: HashSet<String>,

    pub allow_taints: bool,
}

impl ScopeContext {
    pub fn new(function_context: FunctionContext) -> Self {
        Self {
            vars_in_scope: BTreeMap::new(),
            cond_referenced_var_ids: HashSet::new(),
            assigned_var_ids: HashMap::new(),
            possibly_assigned_var_ids: HashSet::new(),

            inside_conditional: false,
            inside_isset: false,
            inside_unset: false,
            inside_class_exists: false,
            inside_general_use: false,
            inside_return: false,
            inside_throw: false,
            inside_assignment: false,
            inside_try: false,

            check_classes: true,
            check_variables: true,
            check_methods: true,
            check_consts: true,
            check_functions: true,

            inside_negation: false,

            error_suppressing: false,
            has_returned: false,
            include_location: None,
            phantom_files: HashMap::new(),
            clauses: Vec::new(),
            reconciled_expression_clauses: Vec::new(),

            protected_var_ids: HashSet::new(),
            branch_point: None,
            break_types: Vec::new(),
            inside_loop: false,
            case_scope: None,
            finally_scope: None,
            function_context,
            parent_remove_vars: HashSet::new(),
            allow_taints: true,
        }
    }

    pub fn get_redefined_vars(
        &self,
        new_vars_in_scope: &BTreeMap<String, Rc<TUnion>>,
        include_new_vars: bool, // default false
    ) -> HashMap<String, TUnion> {
        let mut redefined_vars = HashMap::new();

        for (var_id, this_type) in &self.vars_in_scope {
            if let Some(new_type) = new_vars_in_scope.get(var_id) {
                if new_type != this_type {
                    redefined_vars.insert(var_id.clone(), (**this_type).clone());
                }
            } else {
                if include_new_vars {
                    redefined_vars.insert(var_id.clone(), (**this_type).clone());
                }
            }
        }

        redefined_vars
    }

    pub fn get_new_or_updated_var_ids(
        original_context: &Self,
        new_context: &Self,
    ) -> HashSet<String> {
        let mut redefined_var_ids = HashSet::new();

        for (var_id, new_type) in &new_context.vars_in_scope {
            if let Some(original_type) = original_context.vars_in_scope.get(var_id) {
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
        changed_var_ids: &HashSet<String>,
    ) -> (Vec<Rc<Clause>>, Vec<Rc<Clause>>) {
        let mut included_clauses = Vec::new();
        let mut rejected_clauses = Vec::new();

        'outer: for c in clauses {
            if c.wedge {
                included_clauses.push(c.clone());
                continue;
            }

            for (key, _) in &c.possibilities {
                if changed_var_ids.contains(key) {
                    rejected_clauses.push(c.clone());
                    continue 'outer;
                }
            }

            included_clauses.push(c.clone());
        }

        (included_clauses, rejected_clauses)
    }

    pub fn remove_reconciled_clauses(
        clauses: &Vec<Clause>,
        changed_var_ids: &HashSet<String>,
    ) -> (Vec<Clause>, Vec<Clause>) {
        let mut included_clauses = Vec::new();
        let mut rejected_clauses = Vec::new();

        'outer: for c in clauses {
            if c.wedge {
                included_clauses.push(c.clone());
                continue;
            }

            for (key, _) in &c.possibilities {
                if changed_var_ids.contains(key) {
                    rejected_clauses.push(c.clone());
                    continue 'outer;
                }
            }

            included_clauses.push(c.clone());
        }

        (included_clauses, rejected_clauses)
    }

    pub(crate) fn filter_clauses(
        remove_var_id: &String,
        clauses: Vec<Rc<Clause>>,
        new_type: Option<&TUnion>,
        statements_analyzer: Option<&StatementsAnalyzer>,
        tast_info: &mut TastInfo,
    ) -> Vec<Rc<Clause>> {
        let mut clauses_to_keep = Vec::new();

        let new_type_string = if let Some(new_type) = &new_type {
            new_type.get_id()
        } else {
            "".to_string()
        };

        let mut other_clauses = Vec::new();

        'outer: for clause in clauses {
            for (var_id, _) in &clause.possibilities {
                if var_has_root(&var_id, remove_var_id) {
                    break 'outer;
                }
            }

            let keep_clause = if let Some(possibilities) = clause.possibilities.get(remove_var_id) {
                possibilities.len() == 1
                    && possibilities.values().next().unwrap().to_string() == new_type_string
            } else {
                true
            };

            if keep_clause {
                clauses_to_keep.push(clause.clone())
            } else {
                other_clauses.push(clause);
            }
        }

        if let Some(statements_analyzer) = statements_analyzer {
            if let Some(new_type) = &new_type {
                if !new_type.is_mixed() {
                    for clause in other_clauses {
                        let mut type_changed = false;

                        // if the clause contains any possibilities that would be altered
                        // by the new type
                        for (_, assertion) in clause.possibilities.get(remove_var_id).unwrap() {
                            // if we're negating a type, we generally don't need the clause anymore
                            if assertion.has_negation() {
                                type_changed = true;
                                break;
                            }

                            let result_type = assertion_reconciler::reconcile(
                                assertion,
                                Some(&new_type.clone()),
                                false,
                                &None,
                                statements_analyzer,
                                tast_info,
                                false,
                                None,
                                &mut reconciler::ReconciliationStatus::Ok,
                                false,
                                &HashMap::new(),
                            );

                            if result_type.get_id() != new_type_string {
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

        return clauses_to_keep;
    }

    pub(crate) fn remove_var_from_conflicting_clauses(
        &mut self,
        remove_var_id: &String,
        new_type: Option<&TUnion>,
        statements_analyzer: Option<&StatementsAnalyzer>,
        tast_info: &mut TastInfo,
    ) {
        self.clauses = ScopeContext::filter_clauses(
            remove_var_id,
            self.clauses.clone(),
            new_type,
            statements_analyzer,
            tast_info,
        );
        self.parent_remove_vars.insert(remove_var_id.clone());
    }

    pub(crate) fn remove_descendants(
        &mut self,
        remove_var_id: &String,
        existing_type: &TUnion,
        new_type: Option<&TUnion>,
        statements_analyzer: Option<&StatementsAnalyzer>,
        tast_info: &mut TastInfo,
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
            tast_info,
        );

        let keys = self
            .vars_in_scope
            .iter()
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>();

        for var_id in keys {
            if var_has_root(&var_id, remove_var_id) {
                self.vars_in_scope.remove(&var_id);
            }
        }
    }

    pub(crate) fn remove_mutable_object_vars(&mut self) {
        let mut all_retained = true;

        self.vars_in_scope.retain(|var_id, context_type| {
            let retain =
                !context_type.has_mutations || (!var_id.contains("->") && !var_id.contains("::"));

            if !retain {
                all_retained = false;
            }
            retain
        });

        if all_retained {
            return;
        }

        self.clauses.retain(|clause| {
            let mut retain_clause = true;

            for (var_id, _) in &clause.possibilities {
                if var_id.contains("->") || var_id.contains("::") {
                    retain_clause = false;
                }
            }

            retain_clause
        });
    }

    pub(crate) fn has_variable(&mut self, var_name: &String) -> bool {
        lazy_static! {
            static ref EXTRANEOUS_REGEX: Regex = Regex::new("(->|\\[).*$").unwrap();
        }

        let stripped_var = EXTRANEOUS_REGEX.replace(var_name, "");

        if stripped_var != "$this" || var_name != &stripped_var {
            self.cond_referenced_var_ids.insert(var_name.clone());
        }

        self.vars_in_scope.contains_key(var_name)
    }
}

#[inline]
pub fn var_has_root(var_id: &String, root_var_id: &String) -> bool {
    if let Some(pos) = var_id.find(root_var_id) {
        if var_id == root_var_id {
            return false;
        }
        let bytes = var_id.as_bytes();
        if pos > 0 && (bytes[pos - 1] as char) == ':' {
            return false;
        }
        let i = root_var_id.len() + pos;
        return match bytes[i] as char {
            '[' | '-' | ']' => true,
            _ => false,
        };
    }

    false
}
