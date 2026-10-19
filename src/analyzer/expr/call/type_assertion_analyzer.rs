use crate::{
    function_analysis_data::FunctionAnalysisData, scope::BlockContext,
    scope_analyzer::ScopeAnalyzer, statements_analyzer::StatementsAnalyzer,
};
use hakana_code_info::{
    data_flow::graph::{DataFlowGraph, GraphKind},
    functionlike_info::FunctionLikeInfo,
    issue::{Issue, IssueKind},
    t_atomic::{TAtomic, TGenericParam},
    ttype::{
        comparison::{type_comparison_result::TypeComparisonResult, union_type_comparator},
        template::{TemplateResult, inferred_type_replacer},
        type_expander::{self, TypeExpansionOptions},
    },
};
use oxidized::aast;

pub(super) fn check(
    statements_analyzer: &StatementsAnalyzer,
    functionlike_info: &FunctionLikeInfo,
    args: &[aast::Argument<(), ()>],
    template_result: &TemplateResult,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    let Some((offset, asserted_type)) = functionlike_info.type_assertion.as_deref() else {
        return;
    };
    let Some(parameter) = functionlike_info.params.get(*offset) else {
        return;
    };
    let parameter_name = statements_analyzer.interner.lookup(&parameter.name.0);
    let Some(arg) = args.iter().enumerate().find_map(|(i, arg)| {
        let matches = match arg {
            aast::Argument::Anamed(name, _) => name.1 == parameter_name.trim_start_matches('$'),
            _ => i == *offset,
        };
        matches.then(|| arg.to_expr_ref())
    }) else {
        // An omitted or unpacked value has no individual argument to diagnose.
        return;
    };
    let Some(input_type) = analysis_data.get_expr_type(arg.pos()).cloned() else {
        return;
    };
    if input_type.is_nothing() {
        return;
    }

    let TAtomic::TGenericParam(TGenericParam {
        param_name,
        defining_entity,
        ..
    }) = asserted_type.get_single()
    else {
        return;
    };
    if !template_result
        .lower_bounds
        .get(param_name)
        .and_then(|bounds| bounds.get(defining_entity))
        .is_some_and(|bounds| !bounds.is_empty())
    {
        return;
    }

    let codebase = statements_analyzer.codebase;
    let file_path = statements_analyzer.get_file_path();
    let mut asserted_type =
        inferred_type_replacer::replace(asserted_type, template_result, codebase);
    type_expander::expand_union(
        codebase,
        &Some(statements_analyzer.interner),
        file_path,
        &mut asserted_type,
        &TypeExpansionOptions {
            self_class: context.function_context.calling_class,
            expand_hakana_types: false,
            ..Default::default()
        },
        &mut DataFlowGraph::new(GraphKind::FunctionBody),
        &mut 0,
    );

    if asserted_type.is_any() {
        // Failed inference does not prove that the runtime assertion accepts everything.
        return;
    }

    // An unresolved T may be narrower than its bound. For example, an int does
    // not necessarily satisfy T as int. Only identical template members prove
    // containment until the asserted type is concrete.
    let can_prove_containment = (!asserted_type.has_template_types()
        && !asserted_type.has_template_or_static())
        || input_type
            .types
            .iter()
            .all(|t| asserted_type.types.contains(t));

    // These are the same overlap and containment checks used for `is` expressions.
    // Check the annotated type, not the return type: `?T` helpers return null on
    // failure, just like `?as T`, and other helpers return booleans or errors.
    let (kind, frequency) = if !union_type_comparator::can_type_assertion_succeed(
        codebase,
        file_path,
        &input_type,
        &asserted_type,
    ) {
        (IssueKind::ImpossibleTypeComparison, "never")
    } else if can_prove_containment
        && union_type_comparator::is_contained_by(
            codebase,
            file_path,
            &input_type,
            &asserted_type,
            false,
            false,
            true,
            &mut TypeComparisonResult::new(),
        )
    {
        (IssueKind::RedundantTypeComparison, "always")
    } else {
        return;
    };

    analysis_data.maybe_add_issue(
        Issue::new(
            kind,
            format!(
                "Type {} is {} {}",
                input_type.get_id(Some(statements_analyzer.interner)),
                frequency,
                asserted_type.get_id(Some(statements_analyzer.interner)),
            ),
            statements_analyzer.get_hpos(arg.pos()),
            &context.function_context.calling_functionlike_id,
        ),
        statements_analyzer.get_config(),
        statements_analyzer.get_file_path_actual(),
    );
}
