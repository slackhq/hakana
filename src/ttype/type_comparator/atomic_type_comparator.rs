use crate::{get_arrayish_params, get_value_param};
use hakana_reflection_info::{codebase_info::CodebaseInfo, t_atomic::TAtomic};

use super::{
    closure_type_comparator, dict_type_comparator,
    generic_type_comparator::{self, compare_generic_params},
    object_type_comparator, scalar_type_comparator,
    type_comparison_result::TypeComparisonResult,
    union_type_comparator, vec_type_comparator,
};

pub fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    allow_interface_equality: bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    if input_type_part == container_type_part {
        return true;
    }

    if let TAtomic::TTemplateParam { .. }
    | TAtomic::TNamedObject {
        extra_types: Some(_),
        ..
    } = container_type_part
    {
        if let TAtomic::TTemplateParam { .. }
        | TAtomic::TNamedObject {
            extra_types: Some(_),
            ..
        } = input_type_part
        {
            return object_type_comparator::is_shallowly_contained_by(
                codebase,
                input_type_part,
                container_type_part,
                allow_interface_equality,
                atomic_comparison_result,
            );
        }
    }

    if container_type_part.is_mixed() || container_type_part.is_templated_as_mixed(&mut false) {
        if matches!(container_type_part, TAtomic::TNonnullMixed { .. })
            && matches!(input_type_part, TAtomic::TNull { .. })
        {
            return false;
        }

        return true;
    }

    if matches!(container_type_part, TAtomic::TPlaceholder) {
        return true;
    }

    if matches!(input_type_part, TAtomic::TNothing) {
        return true;
    }

    let mut input_type_has_any = false;
    if input_type_part.is_mixed_with_any(&mut input_type_has_any)
        || input_type_part.is_templated_as_mixed(&mut input_type_has_any)
    {
        atomic_comparison_result.type_coerced = Some(true);
        atomic_comparison_result.type_coerced_from_as_mixed = Some(true);
        if input_type_has_any {
            atomic_comparison_result.type_coerced_from_nested_any = Some(true);
        }
        return false;
    }

    if let TAtomic::TNull = input_type_part {
        if let TAtomic::TTemplateParam { as_type, .. } = container_type_part {
            if as_type.is_nullable() || as_type.is_mixed() {
                return true;
            }
        }

        return false;
    }

    if matches!(input_type_part, TAtomic::TNull { .. }) {
        return false;
    }

    if input_type_part.is_some_scalar() && container_type_part.is_some_scalar() {
        return scalar_type_comparator::is_contained_by(
            codebase,
            input_type_part,
            container_type_part,
            allow_interface_equality,
            atomic_comparison_result,
        );
    }

    // handles newtypes (hopefully)
    if let TAtomic::TEnumLiteralCase {
        constraint_type, ..
    } = input_type_part
    {
        return is_contained_by(
            codebase,
            if let Some(enum_type) = &constraint_type {
                &enum_type
            } else {
                &TAtomic::TArraykey
            },
            container_type_part,
            allow_interface_equality,
            atomic_comparison_result,
        );
    }

    if let TAtomic::TClosure { .. } = container_type_part {
        if let TAtomic::TClosure { .. } = input_type_part {
            return closure_type_comparator::is_contained_by(
                codebase,
                input_type_part,
                container_type_part,
                atomic_comparison_result,
            );
        }

        return false;
    }

    if let TAtomic::TNamedObject { .. } = container_type_part {
        if let TAtomic::TDict { .. } = input_type_part {
            if let Some(arrayish_params) = get_arrayish_params(container_type_part, codebase) {
                return self::is_contained_by(
                    codebase,
                    input_type_part,
                    &TAtomic::TDict {
                        key_param: arrayish_params.0,
                        value_param: arrayish_params.1,
                        known_items: None,
                        enum_items: None,
                        non_empty: false,
                        shape_name: None,
                    },
                    allow_interface_equality,
                    atomic_comparison_result,
                );
            }
        } else if let TAtomic::TVec { .. } = input_type_part {
            if let Some(value_param) = get_value_param(container_type_part, codebase) {
                return self::is_contained_by(
                    codebase,
                    input_type_part,
                    &TAtomic::TVec {
                        type_param: value_param,
                        known_items: None,
                        non_empty: false,
                        known_count: None,
                    },
                    allow_interface_equality,
                    atomic_comparison_result,
                );
            }
        } else if let TAtomic::TKeyset { .. } = input_type_part {
            if let Some(arrayish_params) = get_arrayish_params(container_type_part, codebase) {
                return self::is_contained_by(
                    codebase,
                    input_type_part,
                    &TAtomic::TKeyset {
                        type_param: arrayish_params.0,
                    },
                    allow_interface_equality,
                    atomic_comparison_result,
                );
            }
        }
    }

    if let TAtomic::TDict { .. } = container_type_part {
        if let TAtomic::TDict { .. } = input_type_part {
            return dict_type_comparator::is_contained_by(
                codebase,
                input_type_part,
                container_type_part,
                allow_interface_equality,
                atomic_comparison_result,
            );
        }
    }

    if let TAtomic::TVec { .. } = container_type_part {
        if let TAtomic::TVec { .. } = input_type_part {
            return vec_type_comparator::is_contained_by(
                codebase,
                input_type_part,
                container_type_part,
                allow_interface_equality,
                atomic_comparison_result,
            );
        }
    }

    if let TAtomic::TKeyset {
        type_param: container_type_param,
    } = container_type_part
    {
        if let TAtomic::TKeyset {
            type_param: input_type_param,
        } = input_type_part
        {
            return union_type_comparator::is_contained_by(
                codebase,
                input_type_param,
                container_type_param,
                false,
                input_type_param.ignore_falsable_issues,
                allow_interface_equality,
                atomic_comparison_result,
            );
        }
    }

    if let TAtomic::TTypeAlias { name, .. } = &container_type_part {
        if name == "HH\\Lib\\Regex\\Pattern" {
            if let TAtomic::TRegexPattern { .. } = input_type_part {
                return true;
            }
        }
    }

    // TODO handle TEnumCase for enum classes

    if (matches!(input_type_part, TAtomic::TNamedObject { .. })
        || input_type_part.is_templated_as_object())
        && (matches!(container_type_part, TAtomic::TNamedObject { .. })
            || container_type_part.is_templated_as_object())
        && object_type_comparator::is_shallowly_contained_by(
            codebase,
            input_type_part,
            container_type_part,
            allow_interface_equality,
            atomic_comparison_result,
        )
    {
        if matches!(
            container_type_part,
            TAtomic::TNamedObject {
                type_params: Some(_),
                ..
            }
        ) {
            return generic_type_comparator::is_contained_by(
                codebase,
                input_type_part,
                container_type_part,
                allow_interface_equality,
                atomic_comparison_result,
            );
        }

        if let TAtomic::TNamedObject { is_this: true, .. } = container_type_part {
            if let TAtomic::TNamedObject { is_this: false, .. } = input_type_part {
                atomic_comparison_result.type_coerced = Some(true);
                return false;
            }
        }

        return true;
    }

    if let TAtomic::TObject { .. } = input_type_part {
        if let TAtomic::TObject { .. } = container_type_part {
            return true;
        }
    }

    if let TAtomic::TTemplateParam {
        as_type: container_type_extends,
        ..
    } = container_type_part
    {
        if let TAtomic::TTemplateParam {
            as_type: input_type_extends,
            ..
        } = input_type_part
        {
            return union_type_comparator::is_contained_by(
                codebase,
                input_type_extends,
                container_type_extends,
                false,
                input_type_extends.ignore_falsable_issues,
                allow_interface_equality,
                atomic_comparison_result,
            );
        }

        for (_, container_extends_type_part) in container_type_extends.types.iter() {
            if allow_interface_equality
                && is_contained_by(
                    codebase,
                    input_type_part,
                    container_extends_type_part,
                    allow_interface_equality,
                    atomic_comparison_result,
                )
            {
                return true;
            }
        }

        return false;
    }

    // TODO handle conditional container_type_part

    if let TAtomic::TTemplateParam {
        extra_types: input_extra_types,
        as_type: input_extends,
        ..
    } = input_type_part
    {
        if let Some(input_extra_types) = input_extra_types {
            for (_, input_extra_type) in input_extra_types {
                if is_contained_by(
                    codebase,
                    input_extra_type,
                    container_type_part,
                    allow_interface_equality,
                    atomic_comparison_result,
                ) {
                    return true;
                }
            }
        }

        for (_, input_extends_type_part) in input_extends.types.iter() {
            if matches!(input_extends_type_part, TAtomic::TNull { .. })
                && matches!(container_type_part, TAtomic::TNull { .. })
            {
                continue;
            }

            if is_contained_by(
                codebase,
                input_extends_type_part,
                container_type_part,
                allow_interface_equality,
                atomic_comparison_result,
            ) {
                return true;
            }
        }

        return false;
    }

    // TODO: handle $input_type_part instanceof TConditional

    if let TAtomic::TNamedObject {
        name: input_name, ..
    } = input_type_part
    {
        if input_name == "static" {
            if let TAtomic::TNamedObject {
                name: container_name,
                ..
            } = container_type_part
            {
                if container_name == "self" {
                    return true;
                }
            }
        }
    }

    // handle KeyedContainer and Container accepting arrays
    if let TAtomic::TNamedObject {
        name: container_name,
        type_params: Some(container_type_params),
        ..
    } = container_type_part
    {
        if container_name == &"HH\\Container".to_string()
            || container_name == &"HH\\KeyedContainer".to_string()
        {
            let type_params = get_arrayish_params(input_type_part, codebase);

            if let Some(input_type_params) = type_params {
                let mut all_types_contain = true;

                let mut array_comparison_result = TypeComparisonResult::new();
                if container_name == &"HH\\Container".to_string() {
                    if let Some(container_value_param) = container_type_params.get(0) {
                        if !union_type_comparator::is_contained_by(
                            codebase,
                            &input_type_params.1,
                            container_value_param,
                            false,
                            input_type_params.1.ignore_falsable_issues,
                            allow_interface_equality,
                            &mut array_comparison_result,
                        ) && !array_comparison_result
                            .type_coerced_to_literal
                            .unwrap_or(false)
                        {
                            if array_comparison_result
                                .type_coerced_from_nested_mixed
                                .unwrap_or(false)
                            {
                                atomic_comparison_result.type_coerced_from_nested_mixed =
                                    Some(true);
                            }

                            all_types_contain = false;
                        }
                    }
                } else {
                    if let Some(container_key_param) = container_type_params.get(0) {
                        if !union_type_comparator::is_contained_by(
                            codebase,
                            &input_type_params.0,
                            container_key_param,
                            false,
                            input_type_params.0.ignore_falsable_issues,
                            allow_interface_equality,
                            &mut array_comparison_result,
                        ) && !array_comparison_result
                            .type_coerced_to_literal
                            .unwrap_or(false)
                        {
                            if array_comparison_result
                                .type_coerced_from_nested_mixed
                                .unwrap_or(false)
                            {
                                atomic_comparison_result.type_coerced_from_nested_mixed =
                                    Some(true);
                            }

                            all_types_contain = false;
                        }
                    }

                    let mut array_comparison_result = TypeComparisonResult::new();

                    if let Some(container_value_param) = container_type_params.get(1) {
                        if !union_type_comparator::is_contained_by(
                            codebase,
                            &input_type_params.1,
                            container_value_param,
                            false,
                            input_type_params.1.ignore_falsable_issues,
                            allow_interface_equality,
                            &mut array_comparison_result,
                        ) && !array_comparison_result
                            .type_coerced_to_literal
                            .unwrap_or(false)
                        {
                            if array_comparison_result
                                .type_coerced_from_nested_mixed
                                .unwrap_or(false)
                            {
                                atomic_comparison_result.type_coerced_from_nested_mixed =
                                    Some(true);
                            }

                            all_types_contain = false;
                        }
                    }
                }

                return all_types_contain;
            }

            return false;
        }
    }

    if let TAtomic::TNamedObject {
        name: container_name,
        ..
    } = container_type_part
    {
        if container_name == "XHPChild" {
            if let TAtomic::TString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TFloat
            | TAtomic::TNum = input_type_part
            {
                return true;
            }
        }
    }

    if let TAtomic::TObject { .. } = container_type_part {
        if let TAtomic::TNamedObject { .. } = input_type_part {
            return true;
        }
    }

    // not sure if this will ever get hit. Maybe it belongs inside ObjectComparator
    if let TAtomic::TNamedObject {
        is_this: input_was_static,
        ..
    } = input_type_part
    {
        if let TAtomic::TNamedObject {
            is_this: container_was_static,
            ..
        } = container_type_part
        {
            if *container_was_static && !input_was_static {
                atomic_comparison_result.type_coerced = Some(true);
                return false;
            }
        }
    }

    if let TAtomic::TObject { .. } = input_type_part {
        if let TAtomic::TNamedObject { .. } = container_type_part {
            atomic_comparison_result.type_coerced = Some(true);
            return false;
        }
    }

    if let TAtomic::TNamedObject {
        name: container_name,
        ..
    } = container_type_part
    {
        if let TAtomic::TNamedObject {
            name: input_name, ..
        } = input_type_part
        {
            if (codebase.class_exists(container_name)
                && codebase.class_extends_or_implements(container_name, input_name))
                || (codebase.interface_exists(container_name)
                    && codebase.interface_extends(container_name, input_name))
            {
                atomic_comparison_result.type_coerced = Some(true);

                return false;
            }
        }
    }

    if let TAtomic::TTypeAlias {
        name: container_name,
        type_params: container_type_params,
        ..
    } = container_type_part
    {
        if let TAtomic::TTypeAlias {
            name: input_name,
            type_params: input_type_params,
            ..
        } = input_type_part
        {
            if input_name == container_name {
                match (input_type_params, container_type_params) {
                    (None, None) => return true,
                    (None, Some(_)) => return false,
                    (Some(_), None) => return false,
                    (Some(input_type_params), Some(container_type_params)) => {
                        let mut all_types_contain = true;
                        for (i, input_param) in input_type_params.iter().enumerate() {
                            if let Some(container_param) = container_type_params.get(i) {
                                compare_generic_params(
                                    codebase,
                                    input_type_part,
                                    input_name,
                                    input_param,
                                    container_name,
                                    container_param,
                                    i,
                                    allow_interface_equality,
                                    &mut all_types_contain,
                                    atomic_comparison_result,
                                );
                            } else {
                                all_types_contain = false;
                            }
                        }

                        return all_types_contain;
                    }
                }
            }
        }

        if container_name == "HH\\FormatString" {
            if let TAtomic::TString { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TStringWithFlags { .. } = input_type_part
            {
                // todo maybe more specific checks for the type of format string
                return true;
            }
        }
    }

    if let TAtomic::TTypeAlias {
        name: input_name, ..
    } = input_type_part
    {
        if input_name == "HH\\FormatString" {
            if let TAtomic::TString { .. } = container_type_part {
                return true;
            }
        }

        if let Some(typedef_info) = codebase.type_definitions.get(input_name) {
            if let Some(as_type) = &typedef_info.as_type {
                return is_contained_by(
                    codebase,
                    as_type.get_single(),
                    container_type_part,
                    allow_interface_equality,
                    atomic_comparison_result,
                );
            }
        }
    }

    return input_type_part.get_key() == container_type_part.get_key();
}

pub(crate) fn can_be_identical(
    codebase: &CodebaseInfo,
    type1_part: &TAtomic,
    type2_part: &TAtomic,
    allow_interface_equality: bool,
) -> bool {
    if (type1_part.is_vec() && type2_part.is_non_empty_vec())
        || (type2_part.is_vec() && type1_part.is_non_empty_vec())
    {
        return union_type_comparator::can_expression_types_be_identical(
            codebase,
            type1_part.get_vec_param().unwrap(),
            type2_part.get_vec_param().unwrap(),
            allow_interface_equality,
        );
    }

    if (type1_part.is_dict() && type2_part.is_non_empty_dict())
        || (type2_part.is_dict() && type1_part.is_non_empty_dict())
    {
        let type1_dict_params = type1_part.get_dict_params().unwrap();
        let type2_dict_params = type1_part.get_dict_params().unwrap();

        return union_type_comparator::can_expression_types_be_identical(
            codebase,
            type1_dict_params.0,
            type2_dict_params.0,
            allow_interface_equality,
        ) && union_type_comparator::can_expression_types_be_identical(
            codebase,
            type1_dict_params.1,
            type2_dict_params.1,
            allow_interface_equality,
        );
    }

    let mut first_comparison_result = TypeComparisonResult::new();
    let mut second_comparison_result = TypeComparisonResult::new();

    return is_contained_by(
        codebase,
        type1_part,
        type2_part,
        allow_interface_equality,
        &mut first_comparison_result,
    ) || is_contained_by(
        codebase,
        type2_part,
        type1_part,
        allow_interface_equality,
        &mut second_comparison_result,
    ) || (first_comparison_result.type_coerced.unwrap_or(false)
        && second_comparison_result.type_coerced.unwrap_or(false));
}
