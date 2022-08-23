use super::{atomic_type_comparator, type_comparison_result::TypeComparisonResult};
use hakana_reflection_info::{codebase_info::CodebaseInfo, t_atomic::TAtomic};

pub fn is_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    allow_interface_equality: bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    // compare identical types

    if matches!(container_type_part, TAtomic::TInt) && matches!(input_type_part, TAtomic::TInt) {
        return true;
    }

    if matches!(container_type_part, TAtomic::TFloat) && matches!(input_type_part, TAtomic::TFloat)
    {
        return true;
    }

    if matches!(container_type_part, TAtomic::TString)
        && matches!(input_type_part, TAtomic::TString)
    {
        return true;
    }

    if matches!(
        container_type_part,
        TAtomic::TStringWithFlags(false, true, false)
    ) && matches!(input_type_part, TAtomic::TStringWithFlags(false, true, _))
    {
        return true;
    }

    if matches!(
        container_type_part,
        TAtomic::TStringWithFlags(true, false, false)
    ) && matches!(input_type_part, TAtomic::TStringWithFlags(true, false, _))
    {
        return true;
    }

    if matches!(
        container_type_part,
        TAtomic::TStringWithFlags(false, true, true)
    ) && matches!(
        input_type_part,
        TAtomic::TStringWithFlags(false, true, true)
    ) {
        return true;
    }

    if matches!(
        container_type_part,
        TAtomic::TStringWithFlags(true, false, true)
    ) && matches!(
        input_type_part,
        TAtomic::TStringWithFlags(true, false, true)
    ) {
        return true;
    }

    if matches!(container_type_part, TAtomic::TArraykey)
        && matches!(input_type_part, TAtomic::TArraykey)
    {
        return true;
    }

    if matches!(container_type_part, TAtomic::TFalse) && matches!(input_type_part, TAtomic::TFalse)
    {
        return true;
    }

    if matches!(container_type_part, TAtomic::TTrue) && matches!(input_type_part, TAtomic::TTrue) {
        return true;
    }

    if matches!(container_type_part, TAtomic::TBool)
        && matches!(
            input_type_part,
            TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse
        )
    {
        return true;
    }

    if matches!(container_type_part, TAtomic::TNum)
        && matches!(
            input_type_part,
            TAtomic::TNum | TAtomic::TFloat | TAtomic::TInt | TAtomic::TLiteralInt { .. }
        )
    {
        return true;
    }

    if let TAtomic::TLiteralClassname {
        name: container_name,
        ..
    } = container_type_part
    {
        if let TAtomic::TLiteralClassname {
            name: input_name, ..
        } = input_type_part
        {
            return input_name == container_name;
        }
    }

    if let TAtomic::TLiteralString {
        value: container_value,
        ..
    } = container_type_part
    {
        if let TAtomic::TLiteralString {
            value: input_value, ..
        } = input_type_part
        {
            return input_value == container_value;
        }
    }

    if let TAtomic::TLiteralInt {
        value: container_value,
        ..
    } = container_type_part
    {
        if let TAtomic::TLiteralInt {
            value: input_value, ..
        } = input_type_part
        {
            return input_value == container_value;
        }
    }

    if let TAtomic::TEnum {
        name: container_name,
    } = container_type_part
    {
        if let TAtomic::TEnum { name: input_name } = input_type_part {
            return container_name == input_name;
        }

        if let TAtomic::TEnumLiteralCase {
            enum_name: input_name,
            ..
        } = input_type_part
        {
            return container_name == input_name;
        }

        // check if a string matches an enum case
        if let TAtomic::TLiteralString { value: input_value } = input_type_part {
            if let Some(c) = codebase.classlike_infos.get(container_name) {
                for (_, const_storage) in &c.constants {
                    if let Some(inferred_enum_type) = &const_storage.inferred_type {
                        if let Some(inferred_value) =
                            inferred_enum_type.get_single_literal_string_value()
                        {
                            if &inferred_value == input_value {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        return false;
    }

    if let TAtomic::TEnum { name: input_name } = input_type_part {
        if let Some(c) = codebase.classlike_infos.get(input_name) {
            if let Some(enum_type) = &c.enum_type {
                return is_contained_by(
                    codebase,
                    enum_type,
                    container_type_part,
                    allow_interface_equality,
                    atomic_comparison_result,
                );
            }
        }
    }

    if let TAtomic::TEnumLiteralCase { enum_name, .. } = input_type_part {
        if let TAtomic::TEnumLiteralCase { .. } = container_type_part {
            return false;
        }

        let resolved_input_type = if let Some(c) = codebase.classlike_infos.get(enum_name) {
            if let Some(enum_type) = &c.enum_type {
                enum_type
            } else {
                return false;
            }
        } else {
            return false;
        };

        return is_contained_by(
            codebase,
            resolved_input_type,
            container_type_part,
            allow_interface_equality,
            atomic_comparison_result,
        );
    }

    if let TAtomic::TEnumLiteralCase {
        enum_name: container_name,
        member_name,
    } = container_type_part
    {
        // check if a string matches an enum case
        if let TAtomic::TLiteralString { value: input_value } = input_type_part {
            if let Some(c) = codebase.classlike_infos.get(container_name) {
                if let Some(inferred_enum_type) =
                    &c.constants.get(member_name).unwrap().inferred_type
                {
                    if let Some(inferred_value) =
                        inferred_enum_type.get_single_literal_string_value()
                    {
                        if &inferred_value == input_value {
                            return true;
                        }
                    }
                }
            }
        }

        return false;
    }

    // compare non-identical types

    if matches!(container_type_part, TAtomic::TString) && input_type_part.is_string_subtype() {
        return true;
    }

    if matches!(input_type_part, TAtomic::TString) && container_type_part.is_string_subtype() {
        atomic_comparison_result.type_coerced = Some(true);
        if matches!(container_type_part, TAtomic::TLiteralString { .. }) {
            atomic_comparison_result.type_coerced_to_literal = Some(true);
        }
        return false;
    }

    if matches!(container_type_part, TAtomic::TInt)
        && matches!(input_type_part, TAtomic::TLiteralInt { .. })
    {
        return true;
    }

    if matches!(input_type_part, TAtomic::TInt)
        && matches!(container_type_part, TAtomic::TLiteralInt { .. })
    {
        atomic_comparison_result.type_coerced = Some(true);
        atomic_comparison_result.type_coerced_to_literal = Some(true);
        return false;
    }

    if (matches!(input_type_part, TAtomic::TFalse | TAtomic::TTrue))
        && matches!(container_type_part, TAtomic::TBool)
    {
        return true;
    }

    if (matches!(container_type_part, TAtomic::TFalse | TAtomic::TTrue))
        && matches!(input_type_part, TAtomic::TBool)
    {
        atomic_comparison_result.type_coerced = Some(true);
        return false;
    }

    if matches!(container_type_part, TAtomic::TArraykey)
        && (input_type_part.is_int() || input_type_part.is_string())
    {
        return true;
    }

    if matches!(input_type_part, TAtomic::TArraykey)
        && (container_type_part.is_int() || container_type_part.is_string())
    {
        atomic_comparison_result.type_coerced = Some(true);
        return false;
    }

    if matches!(container_type_part, TAtomic::TScalar) && input_type_part.is_some_scalar() {
        return true;
    }

    if matches!(input_type_part, TAtomic::TScalar) && container_type_part.is_some_scalar() {
        atomic_comparison_result.type_coerced = Some(true);
        return false;
    }

    match container_type_part {
        TAtomic::TStringWithFlags(
            container_is_truthy,
            container_is_nonempty,
            container_is_nonspecific_literal,
        ) => match input_type_part {
            TAtomic::TLiteralClassname { .. } | TAtomic::TClassname { .. } => {
                return true;
            }
            TAtomic::TStringWithFlags(
                input_is_truthy,
                input_is_nonempty,
                input_is_nonspecific_literal,
            ) => {
                if (*input_is_truthy || !container_is_truthy)
                    && (*input_is_nonempty || !container_is_nonempty)
                    && (*input_is_nonspecific_literal || *container_is_nonspecific_literal)
                {
                    return true;
                }

                return false;
            }
            TAtomic::TLiteralString { value } => {
                if value == "" {
                    return !container_is_truthy && !container_is_nonempty;
                }

                if value == "0" {
                    return !container_is_truthy;
                }

                return true;
            }
            _ => {}
        },
        _ => {}
    }

    if matches!(input_type_part, TAtomic::TStringWithFlags(false, true, _))
        && matches!(
            container_type_part,
            TAtomic::TLiteralClassname { .. } | TAtomic::TClassname { .. }
        )
    {
        atomic_comparison_result.type_coerced = Some(true);
        return false;
    }

    // classname<Foo> into classname<Bar>
    if let TAtomic::TClassname {
        as_type: container_name,
        ..
    } = container_type_part
    {
        if let TAtomic::TClassname {
            as_type: input_name,
            ..
        } = input_type_part
        {
            return atomic_type_comparator::is_contained_by(
                codebase,
                input_name,
                container_name,
                allow_interface_equality,
                atomic_comparison_result,
            );
        }
    }

    // Foo::class into classname<Bar>
    if let TAtomic::TClassname {
        as_type: container_name,
        ..
    }
    | TAtomic::TTemplateParamClass {
        as_type: container_name,
        ..
    } = container_type_part
    {
        if let TAtomic::TLiteralClassname {
            name: input_name, ..
        } = input_type_part
        {
            return atomic_type_comparator::is_contained_by(
                codebase,
                &TAtomic::TNamedObject {
                    name: input_name.clone(),
                    type_params: None,
                    is_this: false,
                    extra_types: None,
                    remapped_params: false,
                },
                container_name,
                allow_interface_equality,
                atomic_comparison_result,
            );
        }

        if let TAtomic::TClassname {
            as_type: input_as_type,
            ..
        } = input_type_part
        {
            return atomic_type_comparator::is_contained_by(
                codebase,
                &input_as_type,
                container_name,
                allow_interface_equality,
                atomic_comparison_result,
            );
        }
    }

    if let TAtomic::TTemplateParamType { .. } = container_type_part {
        if let TAtomic::TLiteralClassname {
            name: input_name, ..
        } = input_type_part
        {
            return codebase.typedef_exists(input_name);
        }
    }

    if let TAtomic::TTemplateParamType { .. } = input_type_part {
        if let TAtomic::TString = container_type_part {
            return true;
        }
    }

    // classname<Foo> into Bar::class
    if let TAtomic::TClassname {
        as_type: input_name,
        ..
    }
    | TAtomic::TTemplateParamClass {
        as_type: input_name,
        ..
    } = input_type_part
    {
        if let TAtomic::TLiteralClassname {
            name: container_name,
            ..
        } = container_type_part
        {
            if atomic_type_comparator::is_contained_by(
                codebase,
                &TAtomic::TNamedObject {
                    name: container_name.clone(),
                    type_params: None,
                    is_this: false,
                    extra_types: None,
                    remapped_params: false,
                },
                input_name,
                allow_interface_equality,
                atomic_comparison_result,
            ) {
                atomic_comparison_result.type_coerced = Some(true);
            }

            return false;
        }
    }

    false
}
