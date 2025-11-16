use crate::code_location::FilePath;
use crate::ttype::{template::TemplateBound, wrap_atomic};
use crate::{codebase_info::CodebaseInfo, t_atomic::TAtomic, t_union::TUnion};

use super::{atomic_type_comparator, type_comparison_result::TypeComparisonResult};

/// Configuration for type comparison operations
#[derive(Debug, Clone, Copy)]
struct ComparisonConfig {
    ignore_null: bool,
    ignore_false: bool,
    inside_assertion: bool,
}

/// Tracks coercion state during type comparison
#[derive(Debug, Default)]
struct CoercionTracker {
    type_match_found: bool,
    all_type_coerced: Option<bool>,
    all_type_coerced_from_nested_mixed: Option<bool>,
    all_type_coerced_from_nested_any: Option<bool>,
    all_type_coerced_from_as_mixed: Option<bool>,
    some_type_coerced: bool,
    some_type_coerced_from_nested_mixed: bool,
    some_type_coerced_from_nested_any: bool,
}

pub fn is_contained_by(
    codebase: &CodebaseInfo,
    file_path: &FilePath,
    input_type: &TUnion,
    container_type: &TUnion,
    ignore_null: bool,
    ignore_false: bool,
    inside_assertion: bool,
    union_comparison_result: &mut TypeComparisonResult,
) -> bool {
    if input_type == container_type {
        return true;
    }

    let comparison_config = ComparisonConfig {
        ignore_false,
        ignore_null,
        inside_assertion,
    };

    let container_has_template = container_type.has_template_or_static();

    let mut input_atomic_types = input_type.types.iter().collect::<Vec<_>>();

    input_atomic_types.reverse();

    let mut container_atomic_types = container_type.types.iter().collect::<Vec<_>>();

    container_atomic_types.reverse();

    while let Some(input_type_part) = input_atomic_types.pop() {
        match input_type_part {
            TAtomic::TNull { .. } => {
                if comparison_config.ignore_null {
                    continue;
                }
            }
            TAtomic::TFalse { .. } => {
                if comparison_config.ignore_false {
                    continue;
                }
            }
            TAtomic::TTypeVariable { name } => {
                if container_type.is_single() {
                    if let TAtomic::TTypeVariable {
                        name: container_name,
                    } = container_type.get_single()
                    {
                        if container_name == name {
                            continue;
                        }
                    }
                }
                union_comparison_result.type_variable_upper_bounds.push((
                    name.clone(),
                    TemplateBound::new(container_type.clone(), 0, None, None),
                ));

                continue;
            }
            TAtomic::TGenericParam {
                extra_types: None,
                as_type,
                ..
            } => {
                if !container_has_template {
                    input_atomic_types.extend(as_type.types.iter().collect::<Vec<_>>());
                    continue;
                }
            }
            TAtomic::TClassTypeConstant { .. } => continue,
            _ => (),
        }

        if let Some(atomic_union_check_result) = check_atomic_contained_by_union(
            codebase,
            file_path,
            input_type,
            input_type_part,
            container_type,
            comparison_config,
            union_comparison_result,
            &container_atomic_types,
        ) {
            if !atomic_union_check_result.type_match_found {
                if atomic_union_check_result.some_type_coerced {
                    union_comparison_result.type_coerced = Some(true);
                }

                if atomic_union_check_result.some_type_coerced_from_nested_mixed {
                    union_comparison_result.type_coerced_from_nested_mixed = Some(true);

                    if input_type.from_template_default
                        || atomic_union_check_result
                            .all_type_coerced_from_as_mixed
                            .unwrap_or(false)
                    {
                        union_comparison_result.type_coerced_from_as_mixed = Some(true);
                    }
                }

                if atomic_union_check_result.some_type_coerced_from_nested_any {
                    union_comparison_result.type_coerced_from_nested_any = Some(true);
                }

                return false;
            }
        }
    }

    true
}

fn check_atomic_contained_by_union(
    codebase: &CodebaseInfo,
    file_path: &FilePath,
    input_type: &TUnion,
    input_type_part: &TAtomic,
    container_type: &TUnion,
    comparison_config: ComparisonConfig,
    union_comparison_result: &mut TypeComparisonResult,
    container_atomic_types: &Vec<&TAtomic>,
) -> Option<CoercionTracker> {
    let mut coercion_tracker = CoercionTracker::default();

    if let TAtomic::TArraykey { .. } = input_type_part {
        if container_type.has_int() && container_type.has_string() {
            return None;
        }

        for container_atomic_type in container_atomic_types {
            if let TAtomic::TGenericParam { as_type, .. } = container_atomic_type {
                if as_type.is_arraykey() {
                    return None;
                }
            }
        }
    }

    for container_type_part in container_atomic_types {
        if let Some(true) = check_atomic_contained_by_atomic(
            codebase,
            file_path,
            input_type,
            input_type_part,
            container_type,
            comparison_config,
            union_comparison_result,
            &mut coercion_tracker,
            container_type_part,
        ) {
            coercion_tracker.type_match_found = true;
            coercion_tracker.all_type_coerced_from_nested_mixed = Some(false);
            coercion_tracker.all_type_coerced_from_nested_any = Some(false);
            coercion_tracker.all_type_coerced_from_as_mixed = Some(false);
            coercion_tracker.all_type_coerced = Some(false);
        }
    }
    if coercion_tracker.all_type_coerced.unwrap_or(false) {
        union_comparison_result.type_coerced = Some(true);
    }
    if coercion_tracker
        .all_type_coerced_from_nested_mixed
        .unwrap_or(false)
    {
        union_comparison_result.type_coerced_from_nested_mixed = Some(true);

        if input_type.from_template_default
            || coercion_tracker
                .all_type_coerced_from_as_mixed
                .unwrap_or(false)
        {
            union_comparison_result.type_coerced_from_as_mixed = Some(true);
        }
    }
    if coercion_tracker
        .all_type_coerced_from_nested_any
        .unwrap_or(false)
    {
        union_comparison_result.type_coerced_from_nested_any = Some(true);
    }

    Some(coercion_tracker)
}

fn check_atomic_contained_by_atomic(
    codebase: &CodebaseInfo,
    file_path: &FilePath,
    input_type: &TUnion,
    input_type_part: &TAtomic,
    container_type: &TUnion,
    comparison_config: ComparisonConfig,
    union_comparison_result: &mut TypeComparisonResult,
    coercion_tracker: &mut CoercionTracker,
    container_type_part: &&TAtomic,
) -> Option<bool> {
    if comparison_config.ignore_false
        && matches!(container_type_part, TAtomic::TFalse { .. })
        && !matches!(input_type_part, TAtomic::TFalse { .. })
    {
        return None;
    }

    if let TAtomic::TClassTypeConstant { .. } = &container_type_part {
        coercion_tracker.type_match_found = true;

        return None;
    }

    if let TAtomic::TTypeVariable { name } = &container_type_part {
        union_comparison_result.type_variable_lower_bounds.push((
            name.clone(),
            TemplateBound::new(input_type.clone(), 0, None, None),
        ));

        coercion_tracker.type_match_found = true;

        return None;
    }

    let mut atomic_comparison_result = TypeComparisonResult::new();
    let is_atomic_contained_by = atomic_type_comparator::is_contained_by(
        codebase,
        file_path,
        input_type_part,
        container_type_part,
        comparison_config.inside_assertion,
        &mut atomic_comparison_result,
    );

    let mut mixed_from_any = false;

    if (input_type_part.is_mixed_with_any(&mut mixed_from_any)
        || matches!(input_type_part, TAtomic::TArraykey { from_any: true }))
        && input_type.from_template_default
        && atomic_comparison_result
            .type_coerced_from_nested_mixed
            .unwrap_or(false)
    {
        atomic_comparison_result.type_coerced_from_as_mixed = Some(true);
    }

    if atomic_comparison_result.type_coerced_to_literal.is_some() {
        union_comparison_result.type_coerced_to_literal =
            atomic_comparison_result.type_coerced_to_literal;
    }

    if atomic_comparison_result.upcasted_awaitable && !container_type.had_template {
        union_comparison_result.upcasted_awaitable = true;
    }

    if is_atomic_contained_by {
        if let Some(replacement_atomic_type) = atomic_comparison_result.replacement_atomic_type {
            if let Some(ref mut replacement_union_type) =
                union_comparison_result.replacement_union_type
            {
                replacement_union_type.remove_type(input_type_part);
                replacement_union_type.types.push(replacement_atomic_type);
            } else {
                union_comparison_result.replacement_union_type =
                    Some(wrap_atomic(replacement_atomic_type));
            }
        }

        union_comparison_result
            .type_variable_lower_bounds
            .extend(atomic_comparison_result.type_variable_lower_bounds);

        union_comparison_result
            .type_variable_upper_bounds
            .extend(atomic_comparison_result.type_variable_upper_bounds);
    } else {
        if union_comparison_result.type_mismatch_parents.is_none() {
            union_comparison_result.type_mismatch_parents =
                atomic_comparison_result.type_mismatch_parents;
        }

        // Propagate shape field mismatches for better error messages
        union_comparison_result
            .shape_field_mismatches
            .extend(atomic_comparison_result.shape_field_mismatches);
    }

    if atomic_comparison_result.type_coerced.unwrap_or(false) {
        coercion_tracker.some_type_coerced = true;
    }

    if atomic_comparison_result
        .type_coerced_from_nested_mixed
        .unwrap_or(false)
    {
        coercion_tracker.some_type_coerced_from_nested_mixed = true;
    }

    if atomic_comparison_result
        .type_coerced_from_nested_any
        .unwrap_or(false)
    {
        coercion_tracker.some_type_coerced_from_nested_any = true;
    }

    if !atomic_comparison_result.type_coerced.unwrap_or(false)
        || !coercion_tracker.all_type_coerced.unwrap_or(true)
    {
        coercion_tracker.all_type_coerced = Some(false);
    } else {
        coercion_tracker.all_type_coerced = Some(true);
    }

    if !atomic_comparison_result
        .type_coerced_from_nested_mixed
        .unwrap_or(false)
        || !coercion_tracker
            .all_type_coerced_from_nested_mixed
            .unwrap_or(true)
    {
        coercion_tracker.all_type_coerced_from_nested_mixed = Some(false);
    } else {
        coercion_tracker.all_type_coerced_from_nested_mixed = Some(true);
    }

    if !atomic_comparison_result
        .type_coerced_from_nested_any
        .unwrap_or(false)
        || !coercion_tracker
            .all_type_coerced_from_nested_any
            .unwrap_or(true)
    {
        coercion_tracker.all_type_coerced_from_nested_any = Some(false);
    } else {
        coercion_tracker.all_type_coerced_from_nested_any = Some(true);
    }

    Some(is_atomic_contained_by)
}

pub(crate) fn can_be_contained_by(
    codebase: &CodebaseInfo,
    file_path: &FilePath,
    input_type: &TUnion,
    container_type: &TUnion,
    ignore_null: bool,
    ignore_false: bool,
    matching_input_keys: &mut Vec<String>,
) -> bool {
    if container_type.is_mixed() {
        return true;
    }

    if input_type.is_nothing() {
        return true;
    }

    for container_type_part in &container_type.types {
        if matches!(container_type_part, TAtomic::TNull { .. }) && ignore_null {
            continue;
        }

        if matches!(container_type_part, TAtomic::TFalse { .. }) && ignore_false {
            continue;
        }

        for input_type_part in &input_type.types {
            let mut atomic_comparison_result = TypeComparisonResult::new();

            let is_atomic_contained_by = atomic_type_comparator::is_contained_by(
                codebase,
                file_path,
                input_type_part,
                container_type_part,
                false,
                &mut atomic_comparison_result,
            );

            if is_atomic_contained_by
                || atomic_comparison_result
                    .type_coerced_from_nested_mixed
                    .unwrap_or(false)
            {
                matching_input_keys.push(input_type_part.get_key());
            }
        }
    }

    !matching_input_keys.is_empty()
}

pub fn can_expression_types_be_identical(
    codebase: &CodebaseInfo,
    file_path: &FilePath,
    type1: &TUnion,
    type2: &TUnion,
    inside_assertion: bool,
) -> bool {
    if type1.is_mixed() || type2.is_mixed() {
        return true;
    }

    if type1.is_nullable() && type2.is_nullable() {
        return true;
    }

    for type1_part in &type1.types {
        for type2_part in &type2.types {
            if atomic_type_comparator::can_be_identical(
                codebase,
                file_path,
                type1_part,
                type2_part,
                inside_assertion,
            ) {
                return true;
            }
        }
    }

    false
}
