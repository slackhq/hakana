use std::sync::Arc;

use rustc_hash::FxHashMap;

use hakana_reflection_info::{codebase_info::symbols::Symbol, t_union::TUnion};
use indexmap::IndexMap;

pub mod inferred_type_replacer;
pub mod standin_type_replacer;

/**
 * This struct captures the result of running AHA's argument analysis with
 * regard to generic parameters.
 *
 * It captures upper and lower bounds for parameters. Mostly we just care about
 * lower bounds — those are captured when calling a function that expects a
 * non-callable templated argument.
 *
 * Upper bounds are found in callable parameter types. Given a parameter type
 * `callable(T1): void` and an argument typed as `callable(int): void`, `int` will
 * be added as an _upper_ bound for the template param `T1`. This only applies to
 * parameters — given a parameter type `callable(): T2` and an argument typed as
 * `callable(): string`, `string` will be added as a _lower_ bound for the template
 * param `T2`.
 *
 * @internal
 */
#[derive(Clone, Debug)]
pub struct TemplateResult {
    pub template_types: IndexMap<String, FxHashMap<Symbol, Arc<TUnion>>>,
    pub lower_bounds: IndexMap<String, FxHashMap<Symbol, Vec<TemplateBound>>>,
    pub upper_bounds: IndexMap<String, FxHashMap<Symbol, TemplateBound>>,
    /**
     * If set to true then we shouldn't update the template bounds
     */
    pub readonly: bool,
    pub upper_bounds_unintersectable_types: Vec<TUnion>,
}

impl TemplateResult {
    pub fn new(
        template_types: IndexMap<String, FxHashMap<Symbol, Arc<TUnion>>>,
        lower_bounds: IndexMap<String, FxHashMap<Symbol, TUnion>>,
    ) -> TemplateResult {
        let mut new_lower_bounds = IndexMap::new();

        for (k, v) in lower_bounds {
            let mut th = FxHashMap::default();

            for (vk, vv) in v {
                th.insert(vk, vec![TemplateBound::new(vv, 0, None, None)]);
            }

            new_lower_bounds.insert(k, th);
        }
        TemplateResult {
            template_types,
            lower_bounds: new_lower_bounds,
            upper_bounds: IndexMap::new(),
            readonly: false,
            upper_bounds_unintersectable_types: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TemplateBound {
    pub bound_type: TUnion,

    /**
     * This is the depth at which the template appears in a given type.
     *
     * In the type Foo<T, Bar<T, array<T>>> the type T appears at three different depths.
     *
     * The shallowest-appearance of the template takes prominence when inferring the type of T.
     */
    pub appearance_depth: usize,

    /**
     * The argument offset where this template was set
     *
     * In the type Foo<T, string, T> the type appears at argument offsets 0 and 2
     */
    pub arg_offset: Option<usize>,

    /**
     * When non-null, indicates an equality template bound (vs a lower or upper bound)
     */
    pub equality_bound_classlike: Option<String>,
}

impl TemplateBound {
    pub fn new(
        bound_type: TUnion,
        appearance_depth: usize,
        arg_offset: Option<usize>,
        equality_bound_classlike: Option<String>,
    ) -> Self {
        Self {
            bound_type,
            appearance_depth,
            arg_offset,
            equality_bound_classlike,
        }
    }
}
