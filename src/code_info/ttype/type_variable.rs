use std::{borrow::Borrow, sync::Arc};

use hakana_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    classlike_info::Variance,
    code_location::HPos,
    codebase_info::CodebaseInfo,
    t_atomic::{TAtomic, TGenericParam},
    t_union::{TUnion, TypeNode},
};

use super::{intersect_union_types_simple, template::TemplateBound, type_combiner, wrap_atomic};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypeVariableBounds {
    pub lower_bounds: Vec<TemplateBound>,
    pub upper_bounds: Vec<TemplateBound>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubtypeConstraint {
    pub lower: TUnion,
    pub upper: TUnion,
    pub pos: HPos,
}

/// Variables appearing in both directions cannot be solved from lower bounds
/// alone: a later use of an invariant result may still supply an upper bound.
pub fn get_variance(union: &TUnion, codebase: &CodebaseInfo) -> FxHashMap<String, Variance> {
    fn compose(outer: &Variance, inner: &Variance) -> Variance {
        match (outer, inner) {
            (Variance::Invariant, _) | (_, Variance::Invariant) => Variance::Invariant,
            (Variance::Covariant, inner) => inner.clone(),
            (Variance::Contravariant, Variance::Covariant) => Variance::Contravariant,
            (Variance::Contravariant, Variance::Contravariant) => Variance::Covariant,
        }
    }

    fn visit(
        node: TypeNode<'_>,
        variance: &Variance,
        codebase: &CodebaseInfo,
        variables: &mut FxHashMap<String, Variance>,
    ) {
        if let TypeNode::Atomic(atomic) = &node {
            match atomic {
                TAtomic::TTypeVariable { name } => {
                    variables
                        .entry(name.clone())
                        .and_modify(|previous| {
                            if !matches!(
                                (&*previous, variance),
                                (Variance::Covariant, Variance::Covariant)
                                    | (Variance::Contravariant, Variance::Contravariant)
                            ) {
                                *previous = Variance::Invariant;
                            }
                        })
                        .or_insert_with(|| variance.clone());
                    return;
                }
                TAtomic::TNamedObject(crate::t_atomic::TNamedObject {
                    name,
                    type_params: Some(params),
                    ..
                })
                | TAtomic::TReference {
                    name,
                    type_params: Some(params),
                }
                | TAtomic::TTypeAlias {
                    name,
                    type_params: Some(params),
                    ..
                } => {
                    let declared = codebase
                        .classlike_infos
                        .get(name)
                        .map(|info| &info.generic_variance)
                        .or_else(|| {
                            codebase
                                .type_definitions
                                .get(name)
                                .map(|info| &info.generic_variance)
                        });
                    for (offset, param) in params.iter().enumerate() {
                        let param_variance = declared
                            .and_then(|declared| declared.get(&offset))
                            .unwrap_or(&Variance::Invariant);
                        visit(
                            TypeNode::Union(param),
                            &compose(variance, param_variance),
                            codebase,
                            variables,
                        );
                    }
                    return;
                }
                TAtomic::TClosure(closure) => {
                    if let Some(return_type) = &closure.return_type {
                        visit(TypeNode::Union(return_type), variance, codebase, variables);
                    }
                    for param in &closure.params {
                        if let Some(ty) = &param.signature_type {
                            let param_variance = if param.is_inout {
                                Variance::Invariant
                            } else {
                                compose(variance, &Variance::Contravariant)
                            };
                            visit(TypeNode::Union(ty), &param_variance, codebase, variables);
                        }
                    }
                    return;
                }
                TAtomic::TGenericClassname { as_type, .. }
                | TAtomic::TGenericClassPtr { as_type, .. }
                | TAtomic::TGenericTypename { as_type, .. } => {
                    visit(TypeNode::Atomic(as_type), variance, codebase, variables);
                }
                TAtomic::TClassTypeConstant { as_type, .. } => {
                    visit(TypeNode::Union(as_type), variance, codebase, variables);
                }
                TAtomic::TVec(vec) => {
                    if let Some(variadic_type) = &vec.variadic_type {
                        visit(
                            TypeNode::Union(variadic_type),
                            variance,
                            codebase,
                            variables,
                        );
                    }
                }
                _ => {}
            }
        }
        for child in node.get_child_nodes() {
            visit(child, variance, codebase, variables);
        }
    }

    let mut variables = FxHashMap::default();
    visit(
        TypeNode::Union(union),
        &Variance::Covariant,
        codebase,
        &mut variables,
    );
    variables
}

pub fn contains(union: &TUnion) -> bool {
    union.types.iter().any(atomic_contains)
}

fn atomic_contains(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TTypeVariable { .. } => true,
        TAtomic::TNamedObject(object) => object
            .type_params
            .as_ref()
            .is_some_and(|params| params.iter().any(contains)),
        TAtomic::TReference { type_params, .. } => type_params
            .as_ref()
            .is_some_and(|params| params.iter().any(contains)),
        TAtomic::TTypeAlias {
            type_params,
            as_type,
            ..
        } => {
            type_params
                .as_ref()
                .is_some_and(|params| params.iter().any(contains))
                || as_type.as_ref().is_some_and(|ty| contains(ty))
        }
        TAtomic::TVec(vec) => {
            contains(&vec.type_param)
                || vec
                    .known_items
                    .as_ref()
                    .is_some_and(|items| items.values().any(|(_, ty)| contains(ty)))
                || vec.variadic_type.as_ref().is_some_and(|ty| contains(ty))
        }
        TAtomic::TDict(dict) => {
            dict.params
                .as_ref()
                .is_some_and(|(key, value)| contains(key) || contains(value))
                || dict
                    .known_items
                    .as_ref()
                    .is_some_and(|items| items.values().any(|(_, ty)| contains(ty)))
        }
        TAtomic::TKeyset { type_param, .. } => contains(type_param),
        TAtomic::TAwaitable { value } => contains(value),
        TAtomic::TGenericParam(TGenericParam { as_type, .. })
        | TAtomic::TClassTypeConstant { as_type, .. } => contains(as_type),
        TAtomic::TClosure(closure) => {
            closure.return_type.as_ref().is_some_and(|ty| contains(ty))
                || closure
                    .params
                    .iter()
                    .any(|param| param.signature_type.as_ref().is_some_and(|ty| contains(ty)))
        }
        TAtomic::TClassname { as_type }
        | TAtomic::TClassPtr { as_type }
        | TAtomic::TTypename { as_type }
        | TAtomic::TGenericClassname { as_type, .. }
        | TAtomic::TGenericClassPtr { as_type, .. }
        | TAtomic::TGenericTypename { as_type, .. } => atomic_contains(as_type),
        TAtomic::TObjectIntersection { types } => types.iter().any(atomic_contains),
        _ => false,
    }
}

/// Expand the currently known bounds without committing a solution. An argument
/// checked later can still add a lower bound narrower than the expected type.
pub fn resolve(
    union: &TUnion,
    bounds: &FxHashMap<String, impl Borrow<TypeVariableBounds>>,
    codebase: &CodebaseInfo,
) -> TUnion {
    Resolver::new(bounds, codebase).resolve(union)
}

/// Reuse solutions while checking an immutable set of bounds. The borrow keeps
/// the cache from surviving a mutation, and cycle-dependent entries still
/// require the same blocked ancestors before they can be reused.
pub struct Resolver<'a, B> {
    bounds: &'a FxHashMap<String, B>,
    codebase: &'a CodebaseInfo,
    state: ResolutionState,
}

impl<'a, B: Borrow<TypeVariableBounds>> Resolver<'a, B> {
    pub fn new(bounds: &'a FxHashMap<String, B>, codebase: &'a CodebaseInfo) -> Self {
        Self {
            bounds,
            codebase,
            state: ResolutionState::default(),
        }
    }

    pub fn resolve(&mut self, union: &TUnion) -> TUnion {
        let mut result = union.clone();
        if contains(union) {
            resolve_in_place(&mut result, self.bounds, self.codebase, &mut self.state);
        }
        result
    }
}

/// Resolve the receiver of a deferred member lookup, retaining variables in
/// the member's type so the lookup can contribute new bounds for them.
pub fn resolve_deferred(
    constraint: &SubtypeConstraint,
    bounds: &FxHashMap<String, impl Borrow<TypeVariableBounds>>,
    codebase: &CodebaseInfo,
) -> (TUnion, TUnion) {
    let (lower, upper, _) = resolve_deferred_with_dependencies(constraint, bounds, codebase);
    (lower, upper)
}

/// Include every bound read during resolution, even missing variables and
/// dependencies reached indirectly or through cached solutions. A change to
/// one of these bounds may make a deferred comparison contribute new evidence.
pub fn resolve_deferred_with_dependencies(
    constraint: &SubtypeConstraint,
    bounds: &FxHashMap<String, impl Borrow<TypeVariableBounds>>,
    codebase: &CodebaseInfo,
) -> (TUnion, TUnion, FxHashSet<String>) {
    let mut resolver = Resolver::new(bounds, codebase);
    resolver.state.dependencies.push(FxHashSet::default());
    let lower = resolver.resolve(&constraint.lower);
    let mut upper = constraint.upper.clone();
    for atomic in &mut upper.types {
        if let TAtomic::TTypeAlias {
            name: StrId::ENUM_CLASS_LABEL,
            type_params: Some(params),
            ..
        } = atomic
            && let Some(enum_type) = params.first_mut()
        {
            *enum_type = resolver.resolve(enum_type);
        }
    }
    (lower, upper, resolver.state.dependencies.pop().unwrap())
}

#[derive(Default)]
struct ResolutionState {
    visiting: FxHashSet<String>,
    dependencies: Vec<FxHashSet<String>>,
    resolved: FxHashMap<String, Vec<ResolvedVariable>>,
}

struct ResolvedVariable {
    solution: Option<TUnion>,
    dependencies: FxHashSet<String>,
    blocked: FxHashSet<String>,
}

fn resolve_in_place(
    union: &mut TUnion,
    bounds: &FxHashMap<String, impl Borrow<TypeVariableBounds>>,
    codebase: &CodebaseInfo,
    state: &mut ResolutionState,
) {
    // The caller owns a copy already. Moving its children avoids copying every
    // descendant again at each level of a deeply nested type.
    let mut types = Vec::with_capacity(union.types.len());
    for mut atomic in std::mem::take(&mut union.types) {
        if let TAtomic::TTypeVariable { name } = &atomic
            && let Some(solution) = resolve_variable(name, bounds, codebase, state)
        {
            union.ignore_falsable_issues |= solution.ignore_falsable_issues;
            super::extend_dataflow_uniquely(&mut union.parent_nodes, solution.parent_nodes);
            types.extend(solution.types);
            union.had_template = true;
            continue;
        }

        map_children(&mut atomic, &mut |child| {
            resolve_in_place(child, bounds, codebase, state);
        });
        types.push(atomic);
    }
    union.types = type_combiner::combine(types, codebase, false);
}

fn resolve_variable(
    name: &str,
    bounds: &FxHashMap<String, impl Borrow<TypeVariableBounds>>,
    codebase: &CodebaseInfo,
    state: &mut ResolutionState,
) -> Option<TUnion> {
    if let Some(parent) = state.dependencies.last_mut() {
        parent.insert(name.to_owned());
    }
    if let Some(cached) = state.resolved.get(name).and_then(|solutions| {
        solutions.iter().find(|cached| {
            cached
                .dependencies
                .iter()
                .all(|name| state.visiting.contains(name) == cached.blocked.contains(name))
        })
    }) {
        if let Some(parent) = state.dependencies.last_mut() {
            parent.extend(cached.dependencies.iter().cloned());
        }
        return cached.solution.clone();
    }
    let variable_bounds = bounds.get(name)?.borrow();
    if !state.visiting.insert(name.to_owned()) {
        return None;
    }
    state
        .dependencies
        .push(FxHashSet::from_iter([name.to_owned()]));
    let lower_bounds = variable_bounds
        .lower_bounds
        .iter()
        .filter_map(|bound| {
            let mut bound = bound.clone();
            resolve_in_place(&mut bound.bound_type, bounds, codebase, state);
            // Unsolved aliases, including cycles, supply no concrete
            // evidence. Their edges remain in the constraint graph.
            bound
                .bound_type
                .types
                .retain(|ty| !matches!(ty, TAtomic::TTypeVariable { .. }));
            (!bound.bound_type.types.is_empty()).then_some(bound)
        })
        .collect::<Vec<_>>();
    let solution = if !lower_bounds.is_empty() {
        let has_equal_bounds = lower_bounds
            .iter()
            .any(|bound| bound.equality_bound_classlike.is_some());
        // These are subtype constraints, not competing template guesses.
        // Every lower bound contributes, including bounds reached through a
        // nested call at a different appearance depth.
        lower_bounds
            .into_iter()
            .filter(|bound| !has_equal_bounds || bound.equality_bound_classlike.is_some())
            .map(|bound| bound.bound_type)
            .reduce(|left, right| super::add_union_type(left, &right, codebase, false))
    } else {
        let mut upper_type = None;
        for upper in &variable_bounds.upper_bounds {
            let mut upper = upper.bound_type.clone();
            resolve_in_place(&mut upper, bounds, codebase, state);
            if upper.is_mixed() || contains(&upper) {
                continue;
            }
            upper_type = match upper_type {
                None => Some(upper),
                Some(previous) => Some(
                    intersect_union_types_simple(&previous, &upper, codebase)
                        .unwrap_or_else(super::get_nothing),
                ),
            };
        }
        upper_type
    };
    state.visiting.remove(name);
    let dependencies = state.dependencies.pop().unwrap();
    let blocked = dependencies
        .iter()
        .filter(|name| state.visiting.contains(*name))
        .cloned()
        .collect();
    if let Some(parent) = state.dependencies.last_mut() {
        parent.extend(dependencies.iter().cloned());
    }
    // A cycle prunes evidence from active ancestors. Reuse the result only
    // when the same dependencies are blocked by the caller's recursion stack.
    // Record cached dependencies too, so indirect cycles obey the same rule.
    // The cache only lives as long as the immutable borrow of the bounds.
    state
        .resolved
        .entry(name.to_owned())
        .or_default()
        .push(ResolvedVariable {
            solution: solution.clone(),
            dependencies,
            blocked,
        });
    solution
}

/// Visit nested unions as well as the atomic wrappers around class pointers and
/// intersections. Keep all union metadata, including dataflow parents, intact.
fn map_children(atomic: &mut TAtomic, f: &mut impl FnMut(&mut TUnion)) {
    match atomic {
        TAtomic::TNamedObject(object) => {
            if let Some(params) = &mut object.type_params {
                params.iter_mut().for_each(f);
            }
        }
        TAtomic::TReference { type_params, .. } => {
            if let Some(params) = type_params {
                params.iter_mut().for_each(f);
            }
        }
        TAtomic::TTypeAlias {
            type_params,
            as_type,
            ..
        } => {
            if let Some(params) = type_params {
                params.iter_mut().for_each(&mut *f);
            }
            if let Some(as_type) = as_type {
                f(as_type);
            }
        }
        TAtomic::TVec(vec) => {
            f(&mut vec.type_param);
            if let Some(items) = &mut vec.known_items {
                for (_, ty) in items.values_mut() {
                    f(ty);
                }
            }
            if let Some(variadic) = &mut vec.variadic_type {
                f(variadic);
            }
        }
        TAtomic::TDict(dict) => {
            if let Some((key, value)) = &mut dict.params {
                f(key);
                f(value);
            }
            if let Some(items) = &mut dict.known_items {
                for (_, ty) in Arc::make_mut(items).values_mut() {
                    f(Arc::make_mut(ty));
                }
            }
        }
        TAtomic::TKeyset { type_param, .. } => f(type_param),
        TAtomic::TAwaitable { value } => f(value),
        TAtomic::TGenericParam(TGenericParam { as_type, .. })
        | TAtomic::TClassTypeConstant { as_type, .. } => f(as_type),
        TAtomic::TClosure(closure) => {
            for param in &mut closure.params {
                if let Some(ty) = &mut param.signature_type {
                    f(ty);
                }
            }
            if let Some(ty) = &mut closure.return_type {
                f(ty);
            }
        }
        TAtomic::TClassname { as_type }
        | TAtomic::TClassPtr { as_type }
        | TAtomic::TTypename { as_type }
        | TAtomic::TGenericClassname { as_type, .. }
        | TAtomic::TGenericClassPtr { as_type, .. }
        | TAtomic::TGenericTypename { as_type, .. } => {
            let mut ty = wrap_atomic((**as_type).clone());
            f(&mut ty);
            if ty.is_single() {
                **as_type = ty.get_single().clone();
            }
        }
        TAtomic::TObjectIntersection { types } => {
            for atomic in types {
                map_children(atomic, f);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        code_location::FilePath,
        data_flow::node::DataFlowNode,
        ttype::{
            comparison::{type_comparison_result::TypeComparisonResult, union_type_comparator},
            get_int, get_num, get_string,
        },
    };

    fn variable(name: &str) -> TUnion {
        wrap_atomic(TAtomic::TTypeVariable {
            name: name.to_owned(),
        })
    }

    fn bound(ty: TUnion) -> TemplateBound {
        TemplateBound::new(ty, 0, None, None)
    }

    fn bounds(lower: Vec<TUnion>, upper: Vec<TUnion>) -> TypeVariableBounds {
        TypeVariableBounds {
            lower_bounds: lower.into_iter().map(bound).collect(),
            upper_bounds: upper.into_iter().map(bound).collect(),
        }
    }

    fn deferred_constraint(lower: TUnion, enum_type: TUnion) -> SubtypeConstraint {
        SubtypeConstraint {
            lower,
            upper: wrap_atomic(TAtomic::TTypeAlias {
                name: StrId::ENUM_CLASS_LABEL,
                newtype: true,
                type_params: Some(vec![enum_type, variable("Value")]),
                as_type: None,
            }),
            pos: HPos {
                file_path: FilePath(StrId::EMPTY),
                start_offset: 0,
                end_offset: 1,
                start_line: 1,
                end_line: 1,
                start_column: 0,
                end_column: 1,
            },
        }
    }

    #[test]
    fn deferred_dependencies_include_indirect_cycles_and_missing_bounds() {
        let codebase = CodebaseInfo::new();
        let mut bounds = FxHashMap::from_iter([
            ("A".to_owned(), bounds(vec![variable("B")], vec![])),
            (
                "B".to_owned(),
                bounds(vec![variable("A"), variable("Missing"), get_int()], vec![]),
            ),
        ]);
        // Resolving the enum parameter reuses A's cached solution.
        let constraint = deferred_constraint(variable("A"), variable("A"));
        let (lower, _, dependencies) =
            resolve_deferred_with_dependencies(&constraint, &bounds, &codebase);
        assert!(lower.is_int());
        assert_eq!(
            dependencies,
            FxHashSet::from_iter(["A", "B", "Missing"].map(str::to_owned))
        );
        bounds.insert(
            "Missing".to_owned(),
            TypeVariableBounds {
                lower_bounds: vec![bound(get_string())],
                upper_bounds: vec![],
            },
        );
        let (lower, _, _) = resolve_deferred_with_dependencies(&constraint, &bounds, &codebase);
        assert!(lower.types.contains(&TAtomic::TInt));
        assert!(lower.types.contains(&TAtomic::TString));
    }

    #[test]
    fn deferred_enum_resolution_tracks_upper_bounds_and_retains_member_variables() {
        let codebase = CodebaseInfo::new();
        let bounds = FxHashMap::from_iter([
            ("Enum".to_owned(), bounds(vec![], vec![variable("Context")])),
            ("Value".to_owned(), bounds(vec![get_int()], vec![])),
        ]);
        let constraint = deferred_constraint(get_string(), variable("Enum"));
        let (lower, upper, dependencies) =
            resolve_deferred_with_dependencies(&constraint, &bounds, &codebase);
        assert_eq!(lower, constraint.lower);
        assert_eq!(upper, constraint.upper);
        assert_eq!(
            dependencies,
            FxHashSet::from_iter(["Enum", "Context"].map(str::to_owned))
        );
    }

    #[test]
    fn return_variance_composes_callback_parameters() {
        use crate::{functionlike_parameter::FnParameter, t_atomic::TClosure};

        let callback = |input, output| {
            wrap_atomic(TAtomic::TClosure(Box::new(TClosure {
                params: vec![FnParameter {
                    signature_type: Some(Box::new(input)),
                    is_inout: false,
                    is_variadic: false,
                    is_optional: false,
                }],
                return_type: Some(output),
                effects: None,
                closure_id: (FilePath(StrId::EMPTY), 0),
            })))
        };
        let codebase = CodebaseInfo::new();
        let consumer = callback(variable("T"), get_int());
        assert!(matches!(
            get_variance(&consumer, &codebase)["T"],
            Variance::Contravariant
        ));
        assert!(matches!(
            get_variance(&callback(consumer, get_int()), &codebase)["T"],
            Variance::Covariant
        ));
        assert!(matches!(
            get_variance(&callback(variable("T"), variable("T")), &codebase)["T"],
            Variance::Invariant
        ));
    }

    #[test]
    fn invariant_objects_stay_invariant_inside_covariant_collections() {
        let object = wrap_atomic(TAtomic::TNamedObject(crate::t_atomic::TNamedObject {
            name: StrId::VECTOR,
            type_params: Some(vec![variable("T")]),
            is_this: false,
            remapped_params: false,
        }));
        let codebase = CodebaseInfo::new();
        assert!(matches!(
            get_variance(&super::super::get_vec(object), &codebase)["T"],
            Variance::Invariant
        ));
        assert!(matches!(
            get_variance(&super::super::get_vec(variable("T")), &codebase)["T"],
            Variance::Covariant
        ));
    }

    #[test]
    fn contextual_solution_can_be_refined_by_later_arguments() {
        let codebase = CodebaseInfo::new();
        let mut constraints =
            FxHashMap::from_iter([("T".to_owned(), bounds(vec![], vec![get_num()]))]);
        assert_eq!(resolve(&variable("T"), &constraints, &codebase), get_num());
        constraints
            .get_mut("T")
            .unwrap()
            .lower_bounds
            .push(bound(get_int()));
        assert!(resolve(&variable("T"), &constraints, &codebase).is_int());
    }

    #[test]
    fn equality_keeps_the_contextual_union() {
        let codebase = CodebaseInfo::new();
        let expected = TUnion::new(vec![TAtomic::TInt, TAtomic::TNull]);
        let mut constraint = bounds(vec![wrap_atomic(TAtomic::TNull)], vec![expected.clone()]);
        let mut equality = bound(expected.clone());
        equality.equality_bound_classlike = Some(StrId::CONTAINER);
        constraint.lower_bounds.push(equality);
        let constraints = FxHashMap::from_iter([("T".to_owned(), constraint)]);
        assert_eq!(resolve(&variable("T"), &constraints, &codebase), expected);
    }

    #[test]
    fn cycles_need_concrete_evidence_and_do_not_commit_a_solution() {
        let codebase = CodebaseInfo::new();
        let mut constraints = FxHashMap::from_iter([
            ("T".to_owned(), bounds(vec![variable("U")], vec![])),
            ("U".to_owned(), bounds(vec![variable("T")], vec![])),
        ]);
        assert!(contains(&resolve(&variable("T"), &constraints, &codebase)));
        constraints
            .get_mut("U")
            .unwrap()
            .lower_bounds
            .push(bound(get_int()));
        assert!(resolve(&variable("T"), &constraints, &codebase).is_int());
        assert!(resolve(&variable("U"), &constraints, &codebase).is_int());
    }

    #[test]
    fn lower_bounds_at_different_depths_all_contribute() {
        let mut shallow = bound(get_int());
        shallow.appearance_depth = 1;
        let mut nested = bound(get_string());
        nested.appearance_depth = 3;
        let constraints = FxHashMap::from_iter([(
            "T".to_owned(),
            TypeVariableBounds {
                lower_bounds: vec![shallow, nested],
                upper_bounds: vec![],
            },
        )]);
        let result = resolve(&variable("T"), &constraints, &CodebaseInfo::new());
        assert!(result.has_int());
        assert!(result.has_string());
    }

    #[test]
    fn shared_constraint_branches_do_not_expand_exponentially() {
        let codebase = CodebaseInfo::new();
        let mut constraints = FxHashMap::from_iter([
            ("T0".to_owned(), bounds(vec![get_int()], vec![])),
            ("T1".to_owned(), bounds(vec![get_int()], vec![])),
        ]);
        for i in 2..40 {
            constraints.insert(
                format!("T{i}"),
                bounds(
                    vec![
                        variable(&format!("T{}", i - 1)),
                        variable(&format!("T{}", i - 2)),
                    ],
                    vec![],
                ),
            );
        }
        assert!(resolve(&variable("T39"), &constraints, &codebase).is_int());
    }

    #[test]
    fn checking_multiple_bounds_reuses_shared_solutions() {
        let codebase = CodebaseInfo::new();
        let mut constraints =
            FxHashMap::from_iter([("T0".to_owned(), bounds(vec![get_int()], vec![]))]);
        for i in 1..40 {
            constraints.insert(
                format!("T{i}"),
                bounds(vec![variable(&format!("T{}", i - 1))], vec![]),
            );
        }
        let mut resolver = Resolver::new(&constraints, &codebase);
        for i in (0..40).rev() {
            assert!(resolver.resolve(&variable(&format!("T{i}"))).is_int());
        }
        assert_eq!(resolver.state.resolved.len(), 40);
        assert!(
            resolver
                .state
                .resolved
                .values()
                .all(|solutions| solutions.len() == 1)
        );
    }

    #[test]
    fn shared_constraint_branches_with_cycles_do_not_expand_exponentially() {
        let codebase = CodebaseInfo::new();
        let mut constraints = FxHashMap::from_iter([
            (
                "T0".to_owned(),
                bounds(vec![variable("T0"), get_int()], vec![]),
            ),
            (
                "T1".to_owned(),
                bounds(vec![variable("T1"), get_int()], vec![]),
            ),
        ]);
        for i in 2..40 {
            constraints.insert(
                format!("T{i}"),
                bounds(
                    vec![
                        variable(&format!("T{}", i - 1)),
                        variable(&format!("T{}", i - 2)),
                    ],
                    vec![],
                ),
            );
        }
        assert!(resolve(&variable("T39"), &constraints, &codebase).is_int());
    }

    #[test]
    fn cycle_pruned_solutions_are_not_reused_for_other_roots() {
        let codebase = CodebaseInfo::new();
        let constraints = FxHashMap::from_iter([
            (
                "T".to_owned(),
                bounds(vec![variable("U"), get_int()], vec![]),
            ),
            (
                "U".to_owned(),
                bounds(vec![variable("T"), get_string()], vec![]),
            ),
        ]);
        let input = wrap_atomic(TAtomic::TReference {
            name: StrId::CONTAINER,
            type_params: Some(vec![variable("T"), variable("U")]),
        });
        let resolved = resolve(&input, &constraints, &codebase);
        let TAtomic::TReference {
            type_params: Some(params),
            ..
        } = resolved.get_single()
        else {
            panic!("expected a generic reference");
        };
        let expected = TUnion::new(vec![TAtomic::TInt, TAtomic::TString]);
        assert_eq!(params[0], expected);
        assert_eq!(params[1], expected);
    }

    #[test]
    fn batched_resolution_preserves_cycle_context_between_roots() {
        let codebase = CodebaseInfo::new();
        let constraints = FxHashMap::from_iter([
            (
                "T".to_owned(),
                bounds(vec![variable("U"), get_int()], vec![]),
            ),
            (
                "U".to_owned(),
                bounds(vec![variable("T"), get_string()], vec![]),
            ),
        ]);
        let mut resolver = Resolver::new(&constraints, &codebase);
        for name in ["T", "U", "T"] {
            assert_eq!(
                resolver.resolve(&variable(name)),
                resolve(&variable(name), &constraints, &codebase)
            );
        }
    }

    #[test]
    fn resolving_bounds_normalizes_nested_concrete_unions() {
        let constraints = FxHashMap::from_iter([(
            "T".to_owned(),
            bounds(
                vec![wrap_atomic(TAtomic::TReference {
                    name: StrId::CONTAINER,
                    type_params: Some(vec![TUnion::new(vec![
                        TAtomic::TInt,
                        TAtomic::TLiteralInt { value: 1 },
                    ])]),
                })],
                vec![],
            ),
        )]);
        let resolved = resolve(&variable("T"), &constraints, &CodebaseInfo::new());
        let TAtomic::TReference {
            type_params: Some(params),
            ..
        } = resolved.get_single()
        else {
            panic!("expected a generic reference");
        };
        assert_eq!(params[0], get_int());
    }

    #[test]
    fn conflicting_upper_bounds_stay_empty_after_another_bound() {
        let constraints = FxHashMap::from_iter([(
            "T".to_owned(),
            bounds(vec![], vec![get_int(), get_string(), get_num()]),
        )]);
        assert!(resolve(&variable("T"), &constraints, &CodebaseInfo::new()).is_nothing());
    }

    #[test]
    fn substituted_bounds_preserve_false_suppression_and_dataflow() {
        let node = DataFlowNode::get_for_property((StrId::EMPTY, StrId::EMPTY));
        let mut input = TUnion::new(vec![TAtomic::TString, TAtomic::TFalse]);
        input.ignore_falsable_issues = true;
        input.parent_nodes.push(node.clone());
        let constraints = FxHashMap::from_iter([("T".to_owned(), bounds(vec![input], vec![]))]);
        let resolved = resolve(&variable("T"), &constraints, &CodebaseInfo::new());
        assert!(resolved.ignore_falsable_issues);
        assert_eq!(resolved.parent_nodes, vec![node]);
    }

    #[test]
    fn a_union_contributes_one_bound_and_nullable_arms_are_subtracted() {
        let codebase = CodebaseInfo::new();
        let input = TUnion::new(vec![TAtomic::TInt, TAtomic::TNull]);
        let mut result = TypeComparisonResult::new();
        assert!(union_type_comparator::is_contained_by(
            &codebase,
            &FilePath(StrId::EMPTY),
            &input,
            &variable("T"),
            false,
            false,
            false,
            &mut result,
        ));
        assert_eq!(result.type_variable_lower_bounds.len(), 1);
        assert_eq!(result.type_variable_lower_bounds[0].1.bound_type, input);

        let mut nullable = variable("T");
        nullable.types.push(TAtomic::TNull);
        let mut result = TypeComparisonResult::new();
        assert!(union_type_comparator::is_contained_by(
            &codebase,
            &FilePath(StrId::EMPTY),
            &input,
            &nullable,
            false,
            false,
            false,
            &mut result,
        ));
        assert_eq!(result.type_variable_lower_bounds.len(), 1);
        assert!(result.type_variable_lower_bounds[0].1.bound_type.is_int());
    }
}
