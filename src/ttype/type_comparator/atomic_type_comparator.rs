use crate::{get_arrayish_params, get_value_param, wrap_atomic};
use hakana_reflection_info::{
    codebase_info::CodebaseInfo, t_atomic::TAtomic, STR_ANY_ARRAY, STR_BUILTIN_ENUM, STR_CONTAINER,
    STR_ENUM_CLASS_LABEL, STR_FORMAT_STRING, STR_KEYED_CONTAINER, STR_SELF, STR_STATIC,
    STR_XHP_CHILD,
};

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
    inside_assertion: bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    if input_type_part == container_type_part {
        return true;
    }

    if let TAtomic::TGenericParam { .. }
    | TAtomic::TNamedObject {
        extra_types: Some(_),
        ..
    } = container_type_part
    {
        if let TAtomic::TGenericParam { .. }
        | TAtomic::TNamedObject {
            extra_types: Some(_),
            ..
        } = input_type_part
        {
            return object_type_comparator::is_shallowly_contained_by(
                codebase,
                input_type_part,
                container_type_part,
                inside_assertion,
                atomic_comparison_result,
            );
        }
    }

    if container_type_part.is_mixed() || container_type_part.is_templated_as_mixed(&mut false) {
        if matches!(container_type_part, TAtomic::TMixedWithFlags(_, _, _, true))
            && matches!(
                input_type_part,
                TAtomic::TNull { .. }
                    | TAtomic::TMixed
                    | TAtomic::TMixedWithFlags(_, false, _, false)
            )
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
        atomic_comparison_result.type_coerced_from_nested_mixed = Some(true);
        if input_type_has_any {
            atomic_comparison_result.type_coerced_from_nested_any = Some(true);
        }
        return false;
    }

    if let TAtomic::TNull = input_type_part {
        if let TAtomic::TGenericParam { as_type, .. } = container_type_part {
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
            inside_assertion,
            atomic_comparison_result,
        );
    }

    if let TAtomic::TNamedObject {
        name: STR_XHP_CHILD,
        ..
    } = container_type_part
    {
        if input_type_part.is_string()
            || input_type_part.is_int()
            || matches!(
                input_type_part,
                TAtomic::TFloat | TAtomic::TNum | TAtomic::TArraykey { .. }
            )
        {
            return true;
        }

        if let TAtomic::TVec { .. } | TAtomic::TDict { .. } | TAtomic::TKeyset { .. } =
            input_type_part
        {
            let arrayish_params = get_arrayish_params(input_type_part, codebase);

            if let Some(arrayish_params) = arrayish_params {
                return union_type_comparator::is_contained_by(
                    codebase,
                    &arrayish_params.1,
                    &wrap_atomic(container_type_part.clone()),
                    false,
                    false,
                    inside_assertion,
                    atomic_comparison_result,
                );
            }
        }
    }

    if let TAtomic::TNamedObject {
        name: STR_ANY_ARRAY,
        type_params: Some(type_params),
        ..
    } = container_type_part
    {
        if let TAtomic::TVec { .. } | TAtomic::TDict { .. } | TAtomic::TKeyset { .. } =
            input_type_part
        {
            let arrayish_params = get_arrayish_params(input_type_part, codebase);

            if let Some(arrayish_params) = arrayish_params {
                return union_type_comparator::is_contained_by(
                    codebase,
                    &arrayish_params.0,
                    &type_params[0],
                    false,
                    false,
                    inside_assertion,
                    atomic_comparison_result,
                ) && union_type_comparator::is_contained_by(
                    codebase,
                    &arrayish_params.1,
                    &type_params[1],
                    false,
                    false,
                    inside_assertion,
                    atomic_comparison_result,
                );
            }
        }
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
                &TAtomic::TArraykey { from_any: false }
            },
            container_type_part,
            inside_assertion,
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

    if let TAtomic::TNamedObject {
        name: container_name,
        ..
    } = container_type_part
    {
        match input_type_part {
            TAtomic::TDict { .. } => {
                if let Some(arrayish_params) = get_arrayish_params(container_type_part, codebase) {
                    return self::is_contained_by(
                        codebase,
                        input_type_part,
                        &TAtomic::TDict {
                            params: Some((
                                Box::new(arrayish_params.0),
                                Box::new(arrayish_params.1),
                            )),
                            known_items: None,
                            non_empty: false,
                            shape_name: None,
                        },
                        inside_assertion,
                        atomic_comparison_result,
                    );
                }
            }
            TAtomic::TVec { .. } => {
                if let Some(value_param) = get_value_param(container_type_part, codebase) {
                    return self::is_contained_by(
                        codebase,
                        input_type_part,
                        &TAtomic::TVec {
                            type_param: Box::new(value_param),
                            known_items: None,
                            non_empty: false,
                            known_count: None,
                        },
                        inside_assertion,
                        atomic_comparison_result,
                    );
                }
            }
            TAtomic::TKeyset { .. } => {
                if let Some(arrayish_params) = get_arrayish_params(container_type_part, codebase) {
                    return self::is_contained_by(
                        codebase,
                        input_type_part,
                        &TAtomic::TKeyset {
                            type_param: Box::new(arrayish_params.0),
                        },
                        inside_assertion,
                        atomic_comparison_result,
                    );
                }
            }
            TAtomic::TEnum { .. } => {
                return container_name == &STR_BUILTIN_ENUM;
            }
            _ => (),
        }
    }

    if let TAtomic::TDict { .. } = container_type_part {
        if let TAtomic::TDict { .. } = input_type_part {
            return dict_type_comparator::is_contained_by(
                codebase,
                input_type_part,
                container_type_part,
                inside_assertion,
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
                inside_assertion,
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
                inside_assertion,
                atomic_comparison_result,
            );
        }
    }

    if let TAtomic::TObject { .. } = container_type_part {
        if let TAtomic::TNamedObject { .. } = input_type_part {
            return true;
        }
    }

    if let TAtomic::TObject { .. } = input_type_part {
        if let TAtomic::TNamedObject { .. } = container_type_part {
            atomic_comparison_result.type_coerced = Some(true);
            return false;
        }
    }

    // TODO handle TEnumCase for enum classes

    if (matches!(input_type_part, TAtomic::TNamedObject { .. })
        || input_type_part.is_templated_as_object())
        && (matches!(container_type_part, TAtomic::TNamedObject { .. })
            || container_type_part.is_templated_as_object())
    {
        if object_type_comparator::is_shallowly_contained_by(
            codebase,
            input_type_part,
            container_type_part,
            inside_assertion,
            atomic_comparison_result,
        ) {
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
                    inside_assertion,
                    atomic_comparison_result,
                );
            }

            return true;
        }

        return false;
    }

    if let TAtomic::TObject { .. } = input_type_part {
        if let TAtomic::TObject { .. } = container_type_part {
            return true;
        }
    }

    if let TAtomic::TGenericParam {
        as_type: container_type_extends,
        ..
    } = container_type_part
    {
        if let TAtomic::TGenericParam {
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
                inside_assertion,
                atomic_comparison_result,
            );
        }

        for container_extends_type_part in container_type_extends.types.iter() {
            if inside_assertion
                && is_contained_by(
                    codebase,
                    input_type_part,
                    container_extends_type_part,
                    inside_assertion,
                    atomic_comparison_result,
                )
            {
                return true;
            }
        }

        return false;
    }

    // TODO handle conditional container_type_part

    if let TAtomic::TGenericParam {
        extra_types: input_extra_types,
        as_type: input_extends,
        ..
    } = input_type_part
    {
        if let Some(input_extra_types) = input_extra_types {
            for input_extra_type in input_extra_types {
                if is_contained_by(
                    codebase,
                    input_extra_type,
                    container_type_part,
                    inside_assertion,
                    atomic_comparison_result,
                ) {
                    return true;
                }
            }
        }

        for input_extends_type_part in input_extends.types.iter() {
            if matches!(input_extends_type_part, TAtomic::TNull { .. })
                && matches!(container_type_part, TAtomic::TNull { .. })
            {
                continue;
            }

            if is_contained_by(
                codebase,
                input_extends_type_part,
                container_type_part,
                inside_assertion,
                atomic_comparison_result,
            ) {
                return true;
            }
        }

        return false;
    }

    // TODO: handle $input_type_part instanceof TConditional

    if let TAtomic::TNamedObject {
        name: STR_STATIC, ..
    } = input_type_part
    {
        if let TAtomic::TNamedObject {
            name: container_name,
            ..
        } = container_type_part
        {
            if container_name == &STR_SELF {
                return true;
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
        if let STR_CONTAINER | STR_KEYED_CONTAINER | STR_ANY_ARRAY = *container_name {
            let type_params = get_arrayish_params(input_type_part, codebase);

            if let Some(input_type_params) = type_params {
                let mut all_types_contain = true;

                let mut array_comparison_result = TypeComparisonResult::new();
                if *container_name == STR_CONTAINER {
                    if let Some(container_value_param) = container_type_params.get(0) {
                        if !union_type_comparator::is_contained_by(
                            codebase,
                            &input_type_params.1,
                            container_value_param,
                            false,
                            input_type_params.1.ignore_falsable_issues,
                            inside_assertion,
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
                            inside_assertion,
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
                            inside_assertion,
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
        }
    }

    // handle KeyedContainer and Container accepting arrays
    if let TAtomic::TNamedObject {
        name: input_name,
        type_params: Some(input_type_params),
        ..
    } = input_type_part
    {
        if match *input_name {
            STR_CONTAINER | STR_KEYED_CONTAINER | STR_ANY_ARRAY => true,
            _ => false,
        } {
            if let TAtomic::TKeyset { .. } | TAtomic::TVec { .. } | TAtomic::TDict { .. } =
                container_type_part
            {
                atomic_comparison_result.type_coerced = Some(true);

                let container_arrayish_params =
                    get_arrayish_params(container_type_part, codebase).unwrap();

                if *input_name == STR_CONTAINER {
                    if let Some(input_value_param) = input_type_params.get(0) {
                        union_type_comparator::is_contained_by(
                            codebase,
                            &input_value_param,
                            &container_arrayish_params.1,
                            false,
                            input_value_param.ignore_falsable_issues,
                            inside_assertion,
                            atomic_comparison_result,
                        );
                    }
                } else {
                    if let Some(input_key_param) = input_type_params.get(0) {
                        union_type_comparator::is_contained_by(
                            codebase,
                            &input_key_param,
                            &container_arrayish_params.0,
                            false,
                            input_key_param.ignore_falsable_issues,
                            inside_assertion,
                            atomic_comparison_result,
                        );
                    }

                    let mut array_comparison_result = TypeComparisonResult::new();

                    if let Some(input_value_param) = input_type_params.get(1) {
                        union_type_comparator::is_contained_by(
                            codebase,
                            &input_value_param,
                            &container_arrayish_params.1,
                            false,
                            input_value_param.ignore_falsable_issues,
                            inside_assertion,
                            &mut array_comparison_result,
                        );
                    }
                }

                return false;
            }
        }
    }

    if let TAtomic::TNamedObject {
        name: container_name,
        ..
    } = container_type_part
    {
        if container_name == &STR_XHP_CHILD {
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
                                    None,
                                    i,
                                    inside_assertion,
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

        if *container_name == STR_FORMAT_STRING {
            if let TAtomic::TString { .. }
            | TAtomic::TLiteralString { .. }
            | TAtomic::TStringWithFlags { .. } = input_type_part
            {
                // todo maybe more specific checks for the type of format string
                return true;
            }
        }

        if *container_name == STR_ENUM_CLASS_LABEL {
            if let TAtomic::TEnumClassLabel {
                class_name: input_class_name,
                member_name: input_member_name,
            } = input_type_part
            {
                if let Some(container_type_params) = container_type_params {
                    if let (Some(container_enum_param), Some(_)) =
                        (container_type_params.get(0), container_type_params.get(1))
                    {
                        let container_enum_param = container_enum_param.get_single();

                        if let TAtomic::TNamedObject {
                            name: container_enum_name,
                            ..
                        } = container_enum_param
                        {
                            if let Some(input_class_name) = input_class_name {
                                return input_class_name == container_enum_name;
                            } else {
                                let classlike_info =
                                    codebase.classlike_infos.get(container_enum_name).unwrap();
                                return classlike_info.constants.contains_key(input_member_name);
                            }
                        } else {
                            return false;
                        }
                    } else {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }
    }

    if let TAtomic::TTypeAlias {
        name: input_name,
        as_type,
        ..
    } = input_type_part
    {
        if *input_name == STR_FORMAT_STRING {
            if let TAtomic::TString { .. } = container_type_part {
                return true;
            }
        }

        if let Some(as_type) = as_type {
            return is_contained_by(
                codebase,
                as_type.get_single(),
                container_type_part,
                inside_assertion,
                atomic_comparison_result,
            );
        }
    }

    return false;
}

pub(crate) fn can_be_identical<'a>(
    codebase: &'a CodebaseInfo,
    mut type1_part: &'a TAtomic,
    mut type2_part: &'a TAtomic,
    inside_assertion: bool,
) -> bool {
    while let TAtomic::TTypeAlias { as_type, name, .. } = type1_part {
        if let Some(as_type) = as_type {
            type1_part = as_type.get_single();
        } else if inside_assertion {
            let type_alias_info = codebase.type_definitions.get(name).unwrap();
            type1_part = type_alias_info.actual_type.get_single();
        } else {
            break;
        }
    }

    while let TAtomic::TTypeAlias { as_type, name, .. } = type2_part {
        if let Some(as_type) = as_type {
            type2_part = as_type.get_single();
        } else if inside_assertion {
            let type_alias_info = codebase.type_definitions.get(name).unwrap();
            type2_part = type_alias_info.actual_type.get_single();
        } else {
            break;
        }
    }

    if let TAtomic::TClassTypeConstant { .. } = type1_part {
        return true;
    }

    if let TAtomic::TClassTypeConstant { .. } = type2_part {
        return true;
    }

    if let TAtomic::TTypeVariable { .. } = type1_part {
        return true;
    }

    if let TAtomic::TTypeVariable { .. } = type2_part {
        return true;
    }

    if let TAtomic::TEnumLiteralCase {
        enum_name,
        member_name,
        constraint_type: None,
    } = type1_part
    {
        if !matches!(type2_part, TAtomic::TEnum { .. }) {
            let class_const_type = codebase.get_classconst_literal_value(enum_name, member_name);

            if let Some(class_const_type) = class_const_type {
                type1_part = class_const_type;
            } else {
                let enum_info = codebase.classlike_infos.get(enum_name).unwrap();

                if let Some(enum_type) = &enum_info.enum_type {
                    type1_part = enum_type;
                }
            }
        }
    }

    if let TAtomic::TEnumLiteralCase {
        enum_name,
        member_name,
        constraint_type: None,
    } = type2_part
    {
        if !matches!(type1_part, TAtomic::TEnum { .. }) {
            let class_const_type = codebase.get_classconst_literal_value(enum_name, member_name);

            if let Some(class_const_type) = class_const_type {
                type2_part = class_const_type;
            } else {
                let enum_info = codebase.classlike_infos.get(enum_name).unwrap();

                if let Some(enum_type) = &enum_info.enum_type {
                    type2_part = enum_type;
                }
            }
        }
    }

    if let TAtomic::TEnum {
        name,
        base_type: None,
    } = type1_part
    {
        if !matches!(
            type2_part,
            TAtomic::TEnum { .. } | TAtomic::TEnumLiteralCase { .. }
        ) {
            if let Some(enum_info) = codebase.classlike_infos.get(name) {
                if let Some(enum_type) = &enum_info.enum_type {
                    type1_part = enum_type;
                }
            }
        }
    }

    if let TAtomic::TEnum {
        name,
        base_type: None,
    } = type2_part
    {
        if !matches!(
            type1_part,
            TAtomic::TEnum { .. } | TAtomic::TEnumLiteralCase { .. }
        ) {
            let enum_info = codebase.classlike_infos.get(name).unwrap();

            if let Some(enum_type) = &enum_info.enum_type {
                type2_part = enum_type;
            }
        }
    }

    if (type1_part.is_vec() && type2_part.is_non_empty_vec())
        || (type2_part.is_vec() && type1_part.is_non_empty_vec())
    {
        return union_type_comparator::can_expression_types_be_identical(
            codebase,
            type1_part.get_vec_param().unwrap(),
            type2_part.get_vec_param().unwrap(),
            inside_assertion,
        );
    }

    if let (
        TAtomic::TDict {
            params: type_1_dict_params,
            ..
        },
        TAtomic::TDict {
            params: type_2_dict_params,
            ..
        },
    ) = (type1_part, type2_part)
    {
        if type2_part.is_non_empty_dict() || type1_part.is_non_empty_dict() {
            return match (type_1_dict_params, type_2_dict_params) {
                (None, None) | (None, Some(_)) | (Some(_), None) => true,
                (Some(type_1_dict_params), Some(type_2_dict_params)) => {
                    union_type_comparator::can_expression_types_be_identical(
                        codebase,
                        &type_1_dict_params.0,
                        &type_2_dict_params.0,
                        inside_assertion,
                    ) && union_type_comparator::can_expression_types_be_identical(
                        codebase,
                        &type_1_dict_params.1,
                        &type_2_dict_params.1,
                        inside_assertion,
                    )
                }
            };
        }
    }

    let mut first_comparison_result = TypeComparisonResult::new();
    let mut second_comparison_result = TypeComparisonResult::new();

    return is_contained_by(
        codebase,
        type1_part,
        type2_part,
        inside_assertion,
        &mut first_comparison_result,
    ) || is_contained_by(
        codebase,
        type2_part,
        type1_part,
        inside_assertion,
        &mut second_comparison_result,
    ) || (first_comparison_result.type_coerced.unwrap_or(false)
        && second_comparison_result.type_coerced.unwrap_or(false));
}
