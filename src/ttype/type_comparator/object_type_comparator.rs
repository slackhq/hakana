use crate::wrap_atomic;
use hakana_reflection_info::{codebase_info::CodebaseInfo, t_atomic::TAtomic};

use super::{type_comparison_result::TypeComparisonResult, union_type_comparator};

pub(crate) fn is_shallowly_contained_by(
    codebase: &CodebaseInfo,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    allow_interface_equality: bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    let mut intersection_input_types = input_type_part.get_intersection_types();
    intersection_input_types.0.extend(
        intersection_input_types
            .1
            .iter()
            .map(|(k, v)| (k.clone(), v)),
    );

    let mut intersection_container_types = container_type_part.get_intersection_types();
    intersection_container_types.0.extend(
        intersection_container_types
            .1
            .iter()
            .map(|(k, v)| (k.clone(), v)),
    );

    'outer: for (_, intersection_container_type) in intersection_container_types.0.iter() {
        let container_name = match intersection_container_type {
            TAtomic::TNamedObject { name, .. } => Some(name),
            _ => None,
        };

        let container_type_is_interface = if let Some(container_name) = container_name {
            codebase.interface_exists(container_name)
        } else {
            false
        };

        for (input_type_key, intersection_input_type) in intersection_input_types.0.iter() {
            if allow_interface_equality
                && container_type_is_interface
                && !intersection_container_types.0.contains_key(input_type_key)
            {
                continue 'outer;
            }

            if is_intersection_shallowly_contained_by(
                codebase,
                intersection_input_type,
                intersection_container_type,
                allow_interface_equality,
                atomic_comparison_result,
            ) {
                continue 'outer;
            }
        }

        return false;
    }

    true
}

fn is_intersection_shallowly_contained_by(
    codebase: &CodebaseInfo,
    intersection_input_type: &TAtomic,
    intersection_container_type: &TAtomic,
    allow_interface_equality: bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    if let TAtomic::TTemplateParam {
        defining_entity: container_defining_entity,
        param_name: container_param_name,
        from_class: container_param_from_class,
        ..
    } = intersection_container_type
    {
        if let TAtomic::TTemplateParam {
            defining_entity: input_defining_entity,
            from_class: input_param_from_class,
            param_name: input_param_name,
            as_type: input_extends,
            ..
        } = intersection_input_type
        {
            if !allow_interface_equality {
                if !input_param_from_class || !container_param_from_class {
                    if !input_param_from_class
                        && !container_param_from_class
                        && input_defining_entity != container_defining_entity
                    {
                        return true;
                    }

                    for (_, input_as_atomic) in &input_extends.types {
                        // todo use type equality
                        if input_as_atomic == intersection_container_type {
                            return true;
                        }
                    }
                }
            }

            if input_param_name == container_param_name
                && input_defining_entity == container_defining_entity
            {
                return true;
            }

            if input_param_name != container_param_name
                || (input_defining_entity != container_defining_entity
                    && *input_param_from_class
                    && *container_param_from_class)
            {
                if !input_param_from_class && !container_param_from_class {
                    return false;
                }

                if let Some(input_class_storage) =
                    codebase.classlike_infos.get(input_defining_entity)
                {
                    if let Some(defining_entity_params) = &input_class_storage
                        .template_extended_params
                        .get(container_defining_entity)
                    {
                        if let Some(_) = defining_entity_params.get(container_param_name) {
                            return true;
                        }
                    }
                }
            }

            return false;
        }

        return false;
    }

    if let TAtomic::TTemplateParam {
        as_type: input_extends,
        ..
    } = intersection_input_type
    {
        let mut intersection_container_type = intersection_container_type.clone();

        if let TAtomic::TNamedObject {
            ref mut is_this, ..
        } = intersection_container_type
        {
            *is_this = false;
        }

        return union_type_comparator::is_contained_by(
            codebase,
            input_extends,
            &wrap_atomic(intersection_container_type),
            false,
            input_extends.ignore_falsable_issues,
            allow_interface_equality,
            atomic_comparison_result,
        );
    }

    let container_name = match intersection_container_type {
        TAtomic::TNamedObject { name, .. } => name,
        _ => panic!(),
    };

    let container_is_this = match intersection_container_type {
        TAtomic::TNamedObject { is_this, .. } => *is_this,
        _ => false,
    };

    let input_name = match intersection_input_type {
        TAtomic::TNamedObject { name, .. } => name,
        _ => panic!(),
    };

    let input_is_this = match intersection_input_type {
        TAtomic::TNamedObject { is_this, .. } => *is_this,
        _ => false,
    };

    if input_name == container_name {
        if container_is_this && !input_is_this {
            atomic_comparison_result.type_coerced = Some(true);
            return false;
        }

        return true;
    }

    let input_is_interface = codebase.interface_exists(input_name);
    let container_is_interface = codebase.interface_exists(container_name);

    if allow_interface_equality && container_is_interface && input_is_interface {
        return true;
    }

    if codebase.class_exists(input_name)
        && codebase.class_or_interface_exists(container_name)
        && codebase.class_extends_or_implements(input_name, container_name)
    {
        return true;
    }

    if input_is_interface && codebase.interface_extends(input_name, container_name) {
        return true;
    }

    false
}
