use super::{atomic_type_comparator, type_comparison_result::TypeComparisonResult};
use crate::{code_location::FilePath, codebase_info::CodebaseInfo, t_atomic::TAtomic};

pub fn is_contained_by(
    codebase: &CodebaseInfo,
    file_path: &FilePath,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    inside_assertion: bool,
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

    if matches!(container_type_part, TAtomic::TArraykey { .. })
        && matches!(input_type_part, TAtomic::TArraykey { .. })
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

        // Let classname<T> literals returned by `nameof T` be equivalent to string literals.
        // During the migration from classname<T> strings to class<T> pointers,
        // also support class<T> pointers returned by T::class here.
        if let TAtomic::TLiteralClassname { .. } | TAtomic::TLiteralClassPtr { .. } =
            input_type_part
        {
            // TODO: actually resolve the classname, however this requires passing in an interner everywhere
            return true;
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
        ..
    } = container_type_part
    {
        if let TAtomic::TEnum {
            name: input_name, ..
        } = input_type_part
        {
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
                    if let Some(TAtomic::TLiteralString {
                        value: inferred_value,
                    }) = &const_storage.inferred_type
                    {
                        if inferred_value == input_value {
                            return true;
                        }
                    }
                }
            }
        } else if let TAtomic::TLiteralInt { value: input_value } = input_type_part {
            if let Some(c) = codebase.classlike_infos.get(container_name) {
                for (_, const_storage) in &c.constants {
                    if let Some(TAtomic::TLiteralInt {
                        value: inferred_value,
                    }) = &const_storage.inferred_type
                    {
                        if inferred_value == input_value {
                            return true;
                        }
                    }
                }
            }
        }

        return false;
    }

    if let TAtomic::TEnum {
        as_type: input_as_type,
        ..
    } = input_type_part
    {
        let input_as_type = match input_as_type {
            Some(input_as_type) => input_as_type,
            _ => &TAtomic::TArraykey { from_any: false },
        };
        if let TAtomic::TStringWithFlags(..) = container_type_part {
            return is_contained_by(
                codebase,
                file_path,
                input_as_type,
                &TAtomic::TString,
                inside_assertion,
                atomic_comparison_result,
            );
        }

        return atomic_type_comparator::is_contained_by(
            codebase,
            file_path,
            input_as_type,
            container_type_part,
            inside_assertion,
            atomic_comparison_result,
        );
    }

    if let (
        TAtomic::TEnumLiteralCase {
            enum_name: enum_1_name,
            member_name: enum_1_member_name,
            ..
        },
        TAtomic::TEnumLiteralCase {
            enum_name: enum_2_name,
            member_name: enum_2_member_name,
            ..
        },
    ) = (input_type_part, container_type_part)
    {
        if enum_1_name == enum_2_name && enum_1_member_name == enum_2_member_name {
            return true;
        }
    }

    // handles newtypes (hopefully)
    if let TAtomic::TEnumLiteralCase { as_type, .. } = input_type_part {
        if let TAtomic::TEnumLiteralCase { .. } = container_type_part {
            return false;
        }

        return atomic_type_comparator::is_contained_by(
            codebase,
            file_path,
            if let Some(enum_as_type) = &as_type {
                enum_as_type
            } else {
                &TAtomic::TArraykey { from_any: false }
            },
            container_type_part,
            false,
            atomic_comparison_result,
        );
    }

    if let TAtomic::TEnumLiteralCase {
        enum_name: container_name,
        member_name,
        ..
    } = container_type_part
    {
        // check if a string matches an enum case
        if let TAtomic::TLiteralString { value: input_value } = input_type_part {
            if let Some(c) = codebase.classlike_infos.get(container_name) {
                if let Some(TAtomic::TLiteralString {
                    value: inferred_value,
                }) = &c.constants.get(member_name).unwrap().inferred_type
                {
                    if inferred_value == input_value {
                        return true;
                    }
                }
            }
        } else if let TAtomic::TLiteralInt { value: input_value } = input_type_part {
            if let Some(c) = codebase.classlike_infos.get(container_name) {
                if let Some(TAtomic::TLiteralInt {
                    value: inferred_value,
                }) = &c.constants.get(member_name).unwrap().inferred_type
                {
                    if inferred_value == input_value {
                        return true;
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

    if matches!(container_type_part, TAtomic::TArraykey { .. })
        && (input_type_part.is_int() || input_type_part.is_string())
    {
        return true;
    }

    if let TAtomic::TArraykey { from_any } = input_type_part {
        if container_type_part.is_int() || container_type_part.is_string() {
            atomic_comparison_result.type_coerced = Some(true);
            if *from_any {
                atomic_comparison_result.type_coerced_from_nested_mixed = Some(true);
                atomic_comparison_result.type_coerced_from_nested_any = Some(true);
            }
            return false;
        }
    }

    if matches!(container_type_part, TAtomic::TScalar) && input_type_part.is_some_scalar() {
        return true;
    }

    if matches!(input_type_part, TAtomic::TScalar) && container_type_part.is_some_scalar() {
        atomic_comparison_result.type_coerced = Some(true);
        return false;
    }

    if let TAtomic::TStringWithFlags(
        container_is_truthy,
        container_is_nonempty,
        container_is_nonspecific_literal,
    ) = container_type_part
    {
        match input_type_part {
            // During the migration from classname<T> strings to class<T> pointers,
            // support coercing class<T> pointers into string containers.
            // This is governed by the typechecker flag `class_pointer_ban_classname_static_meth`.
            TAtomic::TLiteralClassname { .. }
            | TAtomic::TClassname { .. }
            | TAtomic::TLiteralClassPtr { .. }
            | TAtomic::TClassPtr { .. }
            | TAtomic::TTypename { .. } => {
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
                if value.is_empty() {
                    return !container_is_truthy && !container_is_nonempty;
                }

                if value == "0" {
                    return !container_is_truthy;
                }

                return true;
            }
            _ => {}
        }
    }

    if matches!(input_type_part, TAtomic::TStringWithFlags(false, true, _))
        && matches!(
            container_type_part,
            TAtomic::TLiteralClassname { .. }
                | TAtomic::TClassname { .. }
                | TAtomic::TTypename { .. }
                | TAtomic::TClassPtr { .. }
                | TAtomic::TLiteralClassPtr { .. }
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
        // During the migration from classname<T> strings to class<T> pointers,
        // support coercing class<T> pointers into classname<T> containers.
        // This is governed by the typechecker flag `class_pointer_ban_classname_static_meth`.
        match input_type_part {
            TAtomic::TClassname {
                as_type: input_name,
            }
            | TAtomic::TClassPtr {
                as_type: input_name,
            } => {
                return atomic_type_comparator::is_contained_by(
                    codebase,
                    file_path,
                    input_name,
                    container_name,
                    inside_assertion,
                    atomic_comparison_result,
                );
            }
            TAtomic::TGenericClassname {
                as_type: input_as_type,
                ..
            }
            | TAtomic::TGenericTypename {
                as_type: input_as_type,
                ..
            }
            | TAtomic::TGenericClassPtr {
                as_type: input_as_type,
                ..
            } => {
                return atomic_type_comparator::is_contained_by(
                    codebase,
                    file_path,
                    input_as_type,
                    container_name,
                    inside_assertion,
                    atomic_comparison_result,
                );
            }
            _ => {}
        }
    }

    if let TAtomic::TTypename {
        as_type: container_name,
        ..
    } = container_type_part
    {
        if let TAtomic::TTypename {
            as_type: input_name,
            ..
        } = input_type_part
        {
            return atomic_type_comparator::is_contained_by(
                codebase,
                file_path,
                input_name,
                container_name,
                inside_assertion,
                atomic_comparison_result,
            );
        }

        if let TAtomic::TGenericClassname {
            as_type: input_as_type,
            ..
        }
        | TAtomic::TGenericClassPtr {
            as_type: input_as_type,
            ..
        }
        | TAtomic::TGenericTypename {
            as_type: input_as_type,
            ..
        } = input_type_part
        {
            return atomic_type_comparator::is_contained_by(
                codebase,
                file_path,
                input_as_type,
                container_name,
                inside_assertion,
                atomic_comparison_result,
            );
        }
    }

    // Foo::class or nameof Foo into classname<Bar> or typename<Bar>
    if let TAtomic::TClassname {
        as_type: container_name,
    }
    | TAtomic::TTypename {
        as_type: container_name,
    }
    | TAtomic::TGenericClassname {
        as_type: container_name,
        ..
    }
    | TAtomic::TClassPtr {
        as_type: container_name,
    }
    | TAtomic::TGenericClassPtr {
        as_type: container_name,
        ..
    }
    | TAtomic::TGenericTypename {
        as_type: container_name,
        ..
    } = container_type_part
    {
        // Accept both classname<T> and class<T> here during class pointer migration.
        // The latter will be a typechecker error if class_class_type=true and class_sub_classname=false.
        match input_type_part {
            TAtomic::TLiteralClassname {
                name: input_name, ..
            }
            | TAtomic::TLiteralClassPtr { name: input_name } => {
                // Can't pass off a classname<T> as a class<T>.
                if container_type_part.is_class_ptr() && !input_type_part.is_class_ptr() {
                    return false;
                }

                let input_type = if codebase.enum_exists(input_name) {
                    TAtomic::TEnum {
                        name: *input_name,
                        as_type: None,
                        underlying_type: None,
                    }
                } else if let Some(typedef_info) = codebase.type_definitions.get(input_name) {
                    if let TAtomic::TTypeAlias {
                        name: alias_name, ..
                    } = &**container_name
                    {
                        if alias_name == input_name {
                            return true;
                        }
                    }

                    typedef_info.actual_type.clone().get_single_owned()
                } else {
                    TAtomic::TNamedObject {
                        name: *input_name,
                        type_params: None,
                        is_this: false,
                        extra_types: None,
                        remapped_params: false,
                    }
                };

                return atomic_type_comparator::is_contained_by(
                    codebase,
                    file_path,
                    &input_type,
                    container_name,
                    inside_assertion,
                    atomic_comparison_result,
                );
            }
            TAtomic::TClassname {
                as_type: input_as_type,
                ..
            }
            | TAtomic::TClassPtr {
                as_type: input_as_type,
            } => {
                // Can't pass off a classname<T> as a class<T>.
                if container_type_part.is_class_ptr() && !input_type_part.is_class_ptr() {
                    return false;
                }

                return atomic_type_comparator::is_contained_by(
                    codebase,
                    file_path,
                    input_as_type,
                    container_name,
                    inside_assertion,
                    atomic_comparison_result,
                );
            }
            _ => {}
        }
    }

    if let TAtomic::TGenericTypename {
        as_type: container_name,
        ..
    } = container_type_part
    {
        match input_type_part {
            TAtomic::TLiteralClassname {
                name: input_name, ..
            }
            | TAtomic::TLiteralClassPtr { name: input_name } => {
                return codebase.class_or_interface_exists(input_name)
                    || codebase.typedef_exists(input_name);
            }
            TAtomic::TGenericClassname {
                as_type: input_as_type,
                ..
            }
            | TAtomic::TGenericTypename {
                as_type: input_as_type,
                ..
            }
            | TAtomic::TGenericClassPtr {
                as_type: input_as_type,
                ..
            } => {
                return atomic_type_comparator::is_contained_by(
                    codebase,
                    file_path,
                    input_as_type,
                    container_name,
                    inside_assertion,
                    atomic_comparison_result,
                );
            }
            _ => {}
        }
    }

    if let TAtomic::TGenericTypename { .. } = input_type_part {
        if let TAtomic::TString = container_type_part {
            return true;
        }
    }

    // classname<Foo> into Bar::class
    if let TAtomic::TClassname {
        as_type: input_name,
        ..
    }
    | TAtomic::TClassPtr {
        as_type: input_name,
    }
    | TAtomic::TGenericClassPtr {
        as_type: input_name,
        ..
    }
    | TAtomic::TGenericClassname {
        as_type: input_name,
        ..
    } = input_type_part
    {
        if let TAtomic::TLiteralClassname {
            name: container_name,
            ..
        }
        | TAtomic::TLiteralClassPtr {
            name: container_name,
        } = container_type_part
        {
            if atomic_type_comparator::is_contained_by(
                codebase,
                file_path,
                &TAtomic::TNamedObject {
                    name: *container_name,
                    type_params: None,
                    is_this: false,
                    extra_types: None,
                    remapped_params: false,
                },
                input_name,
                inside_assertion,
                atomic_comparison_result,
            ) {
                atomic_comparison_result.type_coerced = Some(true);
            }

            return false;
        }
    }

    false
}
