use crate::{
    function_analysis_data::FunctionAnalysisData, scope::BlockContext,
    scope_analyzer::ScopeAnalyzer, statements_analyzer::StatementsAnalyzer,
};
use hakana_code_info::{
    codebase_info::CodebaseInfo,
    issue::{Issue, IssueKind},
    t_atomic::{DictKey, TAtomic, TDict, TNamedObject, TVec},
    t_union::{HasTypeNodes, TUnion, TypeNode},
    ttype::{
        combine_union_types,
        comparison::{type_comparison_result::TypeComparisonResult, union_type_comparator},
        get_int, get_string,
        template::TemplateResult,
        type_combiner, type_variable, wrap_atomic,
    },
};
use hakana_parser::aast;
use hakana_reflector::typehint_resolver::get_type_from_hint;
use hakana_str::StrId;
use std::sync::Arc;

/// Reports `HH\FIXME\UNSAFE_CAST<Tin, _>($value)` when `$value` is known to
/// be something narrower than a `Tin`.
///
/// The typechecker accepts any subtype of `Tin` there, so nothing else says
/// that `Tin` has gone stale, and a stale `Tin` hides what the cast does:
/// `UNSAFE_CAST<mixed, int>($i)` on an int does nothing at all.
pub(super) fn check(
    statements_analyzer: &StatementsAnalyzer,
    type_args: &[aast::Targ],
    args: &[aast::Argument],
    template_result: &TemplateResult,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    let (Some(type_arg), Some(arg)) = (type_args.first(), args.first()) else {
        return;
    };
    let arg = arg.to_expr_ref();
    let Some(input_type) = analysis_data.get_expr_type(arg.pos()) else {
        return;
    };
    // The first bound is the one the type argument set.
    let Some(declared_type) = template_result
        .template_types
        .get_index(0)
        .and_then(|(template_name, _)| template_result.lower_bounds.get(template_name))
        .and_then(|bounds| bounds.values().next())
        .and_then(|bounds| bounds.first())
        .map(|bound| &bound.bound_type)
    else {
        return;
    };

    if input_type.is_nothing() {
        return;
    }

    // Everything can be used as a dynamic, without being a subtype of it.
    if contains_dynamic(input_type) || contains_dynamic(declared_type) {
        return;
    }

    // `declared_type` has had its type constants replaced by their bounds,
    // and an int is not a `this::TId as arraykey`. Go back to what was written
    // to see whether there was one.
    let Some(written_type) = get_type_from_hint(
        type_arg.1.1,
        context.function_context.calling_class,
        statements_analyzer.get_type_resolution_context(),
        statements_analyzer.file_analyzer.resolved_names,
        *statements_analyzer.get_file_path(),
        type_arg.1.0.start_offset() as u32,
    ) else {
        return;
    };

    // The comparison below skips over these rather than resolving them, so
    // it cannot prove anything about a type that holds one.
    let is_unresolved = |atomic: &TAtomic| {
        matches!(
            atomic,
            TAtomic::TClassTypeConstant { .. } | TAtomic::TPlaceholder
        )
    };
    if has_node(&written_type, is_unresolved)
        || has_node(input_type, is_unresolved)
        || has_node(declared_type, is_unresolved)
        || type_variable::contains(input_type)
        || type_variable::contains(declared_type)
    {
        return;
    }

    // What is a subtype of an unresolved T, or of `this`, depends on who asks.
    if declared_type.has_template_types()
        || has_node(declared_type, |atomic| {
            matches!(
                atomic,
                TAtomic::TNamedObject(TNamedObject { is_this: true, .. })
            )
        })
    {
        return;
    }

    let codebase = statements_analyzer.codebase;
    let file_path = statements_analyzer.get_file_path();

    // Only a difference the typechecker would see too is worth reporting.
    let input_type = as_typechecker_sees(input_type, codebase);

    let is_strict_subtype = union_type_comparator::is_contained_by(
        codebase,
        file_path,
        &input_type,
        declared_type,
        false,
        false,
        false,
        &mut TypeComparisonResult::new(),
    ) && !union_type_comparator::is_contained_by(
        codebase,
        file_path,
        declared_type,
        &input_type,
        false,
        false,
        false,
        &mut TypeComparisonResult::new(),
    );

    if !is_strict_subtype {
        return;
    }

    analysis_data.maybe_add_issue(
        Issue::new(
            IssueKind::InvalidArgument,
            format!(
                "Argument 1 of {} expects {}, more specific type {} provided",
                statements_analyzer
                    .interner
                    .lookup(&StrId::FIXME_UNSAFE_CAST),
                declared_type.get_id(Some(statements_analyzer.interner)),
                input_type.get_id(Some(statements_analyzer.interner)),
            ),
            statements_analyzer.get_hpos(arg.pos()),
            &context.function_context.calling_functionlike_id,
        ),
        statements_analyzer.get_config(),
        statements_analyzer.get_file_path_actual(),
    );
}

fn has_node(union: &TUnion, predicate: impl Fn(&TAtomic) -> bool) -> bool {
    union.any_child_node(|node| matches!(node, TypeNode::Atomic(atomic) if predicate(atomic)))
}

fn contains_dynamic(union: &TUnion) -> bool {
    let mut nodes = union.get_child_nodes();

    while let Some(node) = nodes.pop() {
        match node {
            TypeNode::Union(union) => union.push_child_nodes(&mut nodes),
            TypeNode::Atomic(TAtomic::TMixedWithFlags(true, _)) => return true,
            // An unbounded T is held as `T as dynamic`. That bound is not
            // something the code asked for.
            TypeNode::Atomic(TAtomic::TGenericParam(_)) => {}
            TypeNode::Atomic(atomic) => atomic.push_child_nodes(&mut nodes),
        }
    }

    false
}

/// The literal 'a' and a vec known to hold two ints are narrower than the
/// string and the vec<int> that the typechecker has for them. This is the
/// type it would have.
fn as_typechecker_sees(union: &TUnion, codebase: &CodebaseInfo) -> TUnion {
    let mut atomics = union
        .types
        .iter()
        .map(|atomic| atomic_as_typechecker_sees(atomic, codebase))
        .collect::<Vec<_>>();

    if atomics.len() > 1 {
        atomics = type_combiner::combine(atomics, codebase, false);
    }

    let mut union = union.clone();
    union.types = atomics;
    union
}

fn atomic_as_typechecker_sees(atomic: &TAtomic, codebase: &CodebaseInfo) -> TAtomic {
    match atomic {
        TAtomic::TLiteralString { .. } | TAtomic::TStringWithFlags(..) => TAtomic::TString,
        TAtomic::TLiteralInt { .. } => TAtomic::TInt,
        TAtomic::TTrue | TAtomic::TFalse => TAtomic::TBool,
        TAtomic::TEnumLiteralCase {
            enum_name,
            as_type,
            underlying_type,
            ..
        } => TAtomic::TEnum {
            name: *enum_name,
            as_type: as_type.clone(),
            underlying_type: underlying_type.clone(),
        },
        TAtomic::TVec(vec) => {
            // The items of a tuple go the way of those of a vec literal. That
            // leaves a tuple a subtype of nothing it was not one of before.
            let mut type_param = (*vec.type_param).clone();
            for (_, item_type) in vec.known_items.iter().flat_map(|items| items.values()) {
                type_param = combine_union_types(&type_param, item_type, codebase, false);
            }

            TAtomic::TVec(TVec {
                type_param: Box::new(as_typechecker_sees(&type_param, codebase)),
                known_items: None,
                known_count: None,
                non_empty: false,
                variadic_type: vec.variadic_type.clone(),
            })
        }
        TAtomic::TDict(dict) => {
            let mut params = dict.params.as_ref().map(|(key_type, value_type)| {
                (
                    as_typechecker_sees(key_type, codebase),
                    as_typechecker_sees(value_type, codebase),
                )
            });
            let mut known_items = None;

            if let Some(items) = &dict.known_items {
                if dict.is_shape {
                    known_items = Some(Arc::new(
                        items
                            .iter()
                            .map(|(key, (possibly_undefined, value_type))| {
                                (
                                    key.clone(),
                                    (
                                        *possibly_undefined,
                                        Arc::new(as_typechecker_sees(value_type, codebase)),
                                    ),
                                )
                            })
                            .collect(),
                    ));
                } else {
                    // what is known of a dict that was written out or built
                    // up key by key
                    for (key, (_, value_type)) in items.iter() {
                        let key_type = match key {
                            DictKey::Int(_) => get_int(),
                            DictKey::String(_) => get_string(),
                            DictKey::Enum(enum_name, _) => wrap_atomic(TAtomic::TEnum {
                                name: *enum_name,
                                as_type: None,
                                underlying_type: None,
                            }),
                        };
                        let value_type = as_typechecker_sees(value_type, codebase);

                        params = Some(match params {
                            Some((key_types, value_types)) => (
                                combine_union_types(&key_types, &key_type, codebase, false),
                                combine_union_types(&value_types, &value_type, codebase, false),
                            ),
                            None => (key_type, value_type),
                        });
                    }
                }
            }

            TAtomic::TDict(TDict {
                known_items,
                params: params
                    .map(|(key_type, value_type)| (Box::new(key_type), Box::new(value_type))),
                non_empty: false,
                shape_name: dict.shape_name,
                is_shape: dict.is_shape,
            })
        }
        TAtomic::TKeyset { type_param, .. } => TAtomic::TKeyset {
            type_param: Box::new(as_typechecker_sees(type_param, codebase)),
            non_empty: false,
        },
        TAtomic::TAwaitable { value } => TAtomic::TAwaitable {
            value: Box::new(as_typechecker_sees(value, codebase)),
        },
        TAtomic::TNamedObject(object) => TAtomic::TNamedObject(TNamedObject {
            type_params: object.type_params.as_ref().map(|type_params| {
                type_params
                    .iter()
                    .map(|type_param| as_typechecker_sees(type_param, codebase))
                    .collect()
            }),
            ..object.clone()
        }),
        _ => atomic.clone(),
    }
}
