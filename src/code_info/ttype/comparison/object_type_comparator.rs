use crate::code_location::FilePath;
use crate::t_atomic::{TGenericParam, TNamedObject};
use crate::ttype::wrap_atomic;
use crate::{GenericParent, codebase_info::CodebaseInfo, t_atomic::TAtomic};

use super::{type_comparison_result::TypeComparisonResult, union_type_comparator};

pub(crate) fn is_shallowly_contained_by(
    codebase: &CodebaseInfo,
    file_path: &FilePath,
    input_type_part: &TAtomic,
    container_type_part: &TAtomic,
    inside_assertion: bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    let mut intersection_input_types = input_type_part.get_intersection_types();
    intersection_input_types
        .0
        .extend(intersection_input_types.1.iter());

    let mut intersection_container_types = container_type_part.get_intersection_types();
    intersection_container_types
        .0
        .extend(intersection_container_types.1.iter());

    'outer: for intersection_container_type in intersection_container_types.0.iter() {
        for intersection_input_type in intersection_input_types.0.iter() {
            if is_intersection_shallowly_contained_by(
                codebase,
                file_path,
                intersection_input_type,
                intersection_container_type,
                inside_assertion,
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
    file_path: &FilePath,
    intersection_input_type: &TAtomic,
    intersection_container_type: &TAtomic,
    inside_assertion: bool,
    atomic_comparison_result: &mut TypeComparisonResult,
) -> bool {
    if let TAtomic::TGenericParam(TGenericParam {
        defining_entity: container_defining_entity,
        param_name: container_param_name,
        ..
    }) = intersection_container_type
    {
        if let TAtomic::TGenericParam(TGenericParam {
            defining_entity: input_defining_entity,
            param_name: input_param_name,
            as_type: input_extends,
            ..
        }) = intersection_input_type
        {
            if input_param_name == container_param_name
                && input_defining_entity == container_defining_entity
            {
                return true;
            }

            match (input_defining_entity, container_defining_entity) {
                (
                    GenericParent::ClassLike(input_defining_class),
                    GenericParent::ClassLike(container_defining_class),
                ) => {
                    if input_defining_class != container_defining_class {
                        if let Some(input_class_storage) =
                            codebase.classlike_infos.get(input_defining_class)
                        {
                            if let Some(defining_entity_params) = &input_class_storage
                                .template_extended_params
                                .get(container_defining_class)
                            {
                                if defining_entity_params.get(container_param_name).is_some() {
                                    return true;
                                }
                            }
                        }
                    }
                }
                (GenericParent::ClassLike(_), _) | (_, GenericParent::ClassLike(_)) => {
                    for input_as_atomic in &input_extends.types {
                        // todo use type equality
                        if input_as_atomic == intersection_container_type {
                            return true;
                        }
                    }
                }
                _ => {
                    if input_param_name != container_param_name {
                        return false;
                    }
                    if input_defining_entity != container_defining_entity {
                        return true;
                    }
                }
            };

            return false;
        }

        return false;
    }

    if let TAtomic::TGenericParam(TGenericParam {
        as_type: input_extends,
        ..
    }) = intersection_input_type
    {
        let mut intersection_container_type = intersection_container_type.clone();

        if let TAtomic::TNamedObject(TNamedObject {
            ref mut is_this, ..
        }) = intersection_container_type
        {
            *is_this = false;
        }

        return union_type_comparator::is_contained_by(
            codebase,
            file_path,
            input_extends,
            &wrap_atomic(intersection_container_type),
            false,
            input_extends.ignore_falsable_issues,
            inside_assertion,
            atomic_comparison_result,
        );
    }

    let (container_name, container_is_this) = match intersection_container_type {
        TAtomic::TNamedObject(TNamedObject { name, is_this, .. }) => (name, *is_this),
        _ => panic!(),
    };

    let (input_name, input_is_this) = match intersection_input_type {
        TAtomic::TNamedObject(TNamedObject { name, is_this, .. }) => (name, *is_this),
        _ => panic!(),
    };

    if container_is_this && !input_is_this && !inside_assertion {
        atomic_comparison_result.type_coerced = Some(true);
        return false;
    }

    if input_name == container_name {
        return true;
    }

    if codebase.class_or_trait_exists(input_name) {
        if codebase.class_or_interface_exists(container_name)
            && codebase.class_extends_or_implements(input_name, container_name)
        {
            return true;
        }

        if codebase.trait_exists(container_name)
            && codebase.class_or_interface_can_use_trait(input_name, container_name)
        {
            return true;
        }
    }

    let input_is_interface = codebase.interface_exists(input_name);

    if input_is_interface && codebase.interface_extends(input_name, container_name) {
        return true;
    }

    if (codebase.class_exists(container_name)
        && codebase.class_extends_or_implements(container_name, input_name))
        || (codebase.interface_exists(container_name)
            && codebase.interface_extends(container_name, input_name))
    {
        atomic_comparison_result.type_coerced = Some(true);
    }

    false
}
