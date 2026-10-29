use std::cell::OnceCell;

use hakana_code_info::{
    GenericParent,
    code_location::HPos,
    codebase_info::CodebaseInfo,
    t_atomic::TAtomic,
    t_union::TUnion,
    ttype::{
        comparison::{type_comparison_result::TypeComparisonResult, union_type_comparator},
        template::{TemplateBound, TemplateResult, inferred_type_replacer},
        type_expander::{self, TypeExpansionOptions},
        type_variable::{self, TypeVariableBounds},
        wrap_atomic,
    },
};
use hakana_str::StrId;
use indexmap::IndexMap;

use crate::{
    function_analysis_data::FunctionAnalysisData, scope_analyzer::ScopeAnalyzer,
    statements_analyzer::StatementsAnalyzer,
};

/// Fresh variables connect the result expected by a caller to the callee's
/// parameters. TemplateResult remains the interface to argument/return inference.
pub(super) struct CallTemplateInference {
    instantiation: TemplateResult,
    variables: Vec<(StrId, GenericParent, String)>,
}

impl CallTemplateInference {
    pub fn new(
        templates: &TemplateResult,
        analysis_data: &mut FunctionAnalysisData,
        statements_analyzer: &StatementsAnalyzer,
    ) -> Self {
        let codebase = statements_analyzer.codebase;
        let mut instantiation = templates.clone();
        // Inherited class arguments can still contain declaration aliases.
        // Expand the substitution once here, so instantiating an already
        // expanded signature cannot reintroduce those aliases.
        for bound in instantiation
            .lower_bounds
            .values_mut()
            .flat_map(|parents| parents.values_mut())
            .flatten()
        {
            type_expander::expand_union(
                codebase,
                &Some(statements_analyzer.interner),
                statements_analyzer.get_file_path(),
                &mut bound.bound_type,
                &TypeExpansionOptions {
                    expand_templates: false,
                    ..Default::default()
                },
                &mut analysis_data.data_flow_graph,
                &mut 0,
            );
        }
        let mut variables = Vec::new();
        for (param_name, entities) in &templates.template_types {
            for (entity, _) in entities {
                // Unbound class parameters also need fresh variables while
                // checking constructor arguments. Passing declaration generics
                // into nested calls would make them rigid contextual bounds.
                if templates
                    .lower_bounds
                    .get(param_name)
                    .is_some_and(|bounds| bounds.contains_key(entity))
                {
                    continue;
                }
                let name = format!("`call_{}", analysis_data.type_variable_bounds.len());
                analysis_data.type_variable_bounds.insert(
                    name.clone(),
                    TypeVariableBounds {
                        lower_bounds: Vec::new(),
                        upper_bounds: Vec::new(),
                    },
                );
                instantiation
                    .lower_bounds
                    .entry(*param_name)
                    .or_default()
                    .insert(
                        *entity,
                        vec![TemplateBound::new(
                            wrap_atomic(TAtomic::TTypeVariable { name: name.clone() }),
                            0,
                            None,
                            None,
                        )],
                    );
                variables.push((*param_name, *entity, name));
            }
        }
        for (param_name, entity, name) in &variables {
            if let Some((_, bound)) = templates.template_types[param_name]
                .iter()
                .find(|(parent, _)| parent == entity)
            {
                let mut bound = inferred_type_replacer::replace(bound, &instantiation, codebase);
                type_expander::expand_union(
                    codebase,
                    &Some(statements_analyzer.interner),
                    statements_analyzer.get_file_path(),
                    &mut bound,
                    &TypeExpansionOptions {
                        expand_templates: false,
                        ..Default::default()
                    },
                    &mut analysis_data.data_flow_graph,
                    &mut 0,
                );
                analysis_data
                    .type_variable_bounds
                    .get_mut(name)
                    .unwrap()
                    .upper_bounds
                    .push(TemplateBound::new(bound, 0, None, None));
            }
        }
        // Class variables supply context for nested expressions. Their actual
        // argument inference and mutable lifetime remain owned by new_analyzer.
        variables.retain(|(_, entity, _)| matches!(entity, GenericParent::FunctionLike(_)));
        Self {
            instantiation,
            variables,
        }
    }

    pub fn instantiate(&self, ty: &TUnion, codebase: &CodebaseInfo) -> TUnion {
        inferred_type_replacer::replace(ty, &self.instantiation, codebase)
    }

    pub fn is_empty(&self) -> bool {
        self.variables.is_empty()
    }

    /// Only equality constraints fix a parameter before its arguments have been
    /// inferred. An upper bound such as num must not replace T in identity(1).
    pub fn known_types(
        &self,
        analysis_data: &FunctionAnalysisData,
        codebase: &CodebaseInfo,
    ) -> TemplateResult {
        let mut known = TemplateResult::new(IndexMap::new(), IndexMap::new());
        for (param_name, entity, name) in &self.variables {
            let bounds = &analysis_data.type_variable_bounds[name];
            if !bounds
                .lower_bounds
                .iter()
                .any(|b| b.equality_bound_classlike.is_some())
            {
                continue;
            }
            let resolved = type_variable::resolve(
                &wrap_atomic(TAtomic::TTypeVariable { name: name.clone() }),
                &analysis_data.type_variable_bounds,
                codebase,
            );
            if !type_variable::contains(&resolved) {
                known
                    .lower_bounds
                    .entry(*param_name)
                    .or_default()
                    .insert(*entity, vec![TemplateBound::new(resolved, 0, None, None)]);
            }
        }
        known
    }

    pub fn finish(
        &self,
        templates: &mut TemplateResult,
        statements_analyzer: &StatementsAnalyzer,
        analysis_data: &mut FunctionAnalysisData,
    ) {
        // Import argument inference without confusing an expected upper bound
        // with a value supplied by an argument.
        for (param_name, entity, name) in &self.variables {
            if let Some(bounds) = templates
                .lower_bounds
                .get(param_name)
                .and_then(|entities| entities.get(entity))
            {
                let variable_bounds = analysis_data.type_variable_bounds.get_mut(name).unwrap();
                for bound in bounds {
                    if !variable_bounds.lower_bounds.contains(bound) {
                        variable_bounds.lower_bounds.push(bound.clone());
                    }
                }
            }
        }
        propagate(statements_analyzer, analysis_data);
        for (param_name, entity, name) in &self.variables {
            let bounds = &analysis_data.type_variable_bounds[name];
            if bounds.lower_bounds.is_empty()
                && bounds.upper_bounds.iter().all(|bound| bound.pos.is_none())
            {
                // Keep the existing unbound-template behavior when neither an
                // argument nor a caller contributed a constraint.
                continue;
            }
            let variable = wrap_atomic(TAtomic::TTypeVariable { name: name.clone() });
            let resolved = if bounds.lower_bounds.is_empty() {
                // An expected type is only an upper bound. Keep the variable
                // live so a later argument/deferred label can supply its value,
                // instead of feeding the expectation back as a lower bound.
                variable
            } else {
                type_variable::resolve(
                    &variable,
                    &analysis_data.type_variable_bounds,
                    statements_analyzer.codebase,
                )
            };
            templates
                .lower_bounds
                .entry(*param_name)
                .or_default()
                .insert(*entity, vec![TemplateBound::new(resolved, 0, None, None)]);
        }
    }
}

pub(super) fn constrain(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    lower: &TUnion,
    upper: &TUnion,
    pos: HPos,
    argument_offset: Option<usize>,
) {
    let mut result = TypeComparisonResult::new();
    if union_type_comparator::is_contained_by(
        statements_analyzer.codebase,
        statements_analyzer.get_file_path(),
        lower,
        upper,
        false,
        false,
        false,
        &mut result,
    ) {
        for (_, bound) in result
            .type_variable_lower_bounds
            .iter_mut()
            .chain(&mut result.type_variable_upper_bounds)
        {
            if argument_offset.is_some() {
                bound.appearance_depth += 1;
                bound.arg_offset = argument_offset;
            }
        }
        analysis_data.add_type_variable_bounds(result, pos);
    }
}

pub(crate) fn propagate(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
) {
    while analysis_data.type_variable_bounds.has_changes()
        || analysis_data.type_variable_constraints_processed
            != analysis_data.type_variable_constraints.len()
    {
        let mut retry_deferred = analysis_data.type_variable_constraints_processed
            != analysis_data.type_variable_constraints.len();
        let mut pending = Vec::new();
        for name in analysis_data.type_variable_bounds.take_changed() {
            let Some(bounds) = analysis_data.type_variable_bounds.get(&name) else {
                continue;
            };
            let mut progress = analysis_data
                .type_variable_bound_progress
                .get(&name)
                .copied()
                .unwrap_or_default();
            let current = (bounds.lower_bounds.len(), bounds.upper_bounds.len());
            if progress == current {
                continue;
            }
            retry_deferred = true;
            // Closure merges may deduplicate bounds; replay after a shrink.
            if progress.0 > current.0 || progress.1 > current.1 {
                progress = (0, 0);
            }
            let upper_has_variables = vec![OnceCell::new(); bounds.upper_bounds.len()];
            for (i, lower) in bounds.lower_bounds.iter().enumerate() {
                let upper_start = if i < progress.0 { progress.1 } else { 0 };
                let lower_has_variables = OnceCell::new();
                for (upper, has_variables) in bounds.upper_bounds[upper_start..]
                    .iter()
                    .zip(&upper_has_variables[upper_start..])
                {
                    let Some(pos) = lower.pos.or(upper.pos) else {
                        continue;
                    };
                    if !*lower_has_variables
                        .get_or_init(|| type_variable::contains(&lower.bound_type))
                        && !*has_variables
                            .get_or_init(|| type_variable::contains(&upper.bound_type))
                    {
                        continue;
                    }
                    pending.push((lower.bound_type.clone(), upper.bound_type.clone(), pos));
                }
            }
            analysis_data
                .type_variable_bound_progress
                .insert(name, current);
        }
        for (lower, upper, pos) in pending {
            let mut result = TypeComparisonResult::new();
            if union_type_comparator::is_contained_by(
                statements_analyzer.codebase,
                statements_analyzer.get_file_path(),
                &lower,
                &upper,
                false,
                false,
                false,
                &mut result,
            ) {
                analysis_data.add_type_variable_bounds(result, pos);
            }
        }

        if !retry_deferred {
            continue;
        }
        analysis_data.type_variable_constraints_processed =
            analysis_data.type_variable_constraints.len();
        for constraint in analysis_data.type_variable_constraints.clone() {
            let (lower, upper) = type_variable::resolve_deferred(
                &constraint,
                &analysis_data.type_variable_bounds,
                statements_analyzer.codebase,
            );
            let mut result = TypeComparisonResult::new();
            if union_type_comparator::is_contained_by(
                statements_analyzer.codebase,
                statements_analyzer.get_file_path(),
                &lower,
                &upper,
                false,
                false,
                false,
                &mut result,
            ) {
                analysis_data.add_type_variable_bounds(result, constraint.pos);
            }
        }
    }
}

pub(crate) fn check_deferred_constraints(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
) {
    use hakana_code_info::issue::{Issue, IssueKind};
    for constraint in analysis_data.type_variable_constraints.clone() {
        let lower = type_variable::resolve(
            &constraint.lower,
            &analysis_data.type_variable_bounds,
            statements_analyzer.codebase,
        );
        let upper = type_variable::resolve(
            &constraint.upper,
            &analysis_data.type_variable_bounds,
            statements_analyzer.codebase,
        );
        let mut comparison = TypeComparisonResult::new();
        let matches = union_type_comparator::is_contained_by(
            statements_analyzer.codebase,
            statements_analyzer.get_file_path(),
            &lower,
            &upper,
            false,
            false,
            false,
            &mut comparison,
        );
        if (!matches || !comparison.deferred_constraints.is_empty())
            && !analysis_data.has_type_error_at(constraint.pos)
        {
            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::InvalidArgument,
                    if !comparison.deferred_constraints.is_empty() {
                        format!(
                            "Cannot infer the expected type for {}",
                            lower.get_id(Some(statements_analyzer.interner))
                        )
                    } else {
                        format!(
                            "Argument type {} does not match {}",
                            lower.get_id(Some(statements_analyzer.interner)),
                            upper.get_id(Some(statements_analyzer.interner))
                        )
                    },
                    constraint.pos,
                    &None,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );
        }
    }
}
