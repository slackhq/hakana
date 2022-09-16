use std::sync::Arc;

use crate::typehint_resolver::get_type_from_hint;
use hakana_file_info::FileSource;
use hakana_reflection_info::{
    class_constant_info::ConstantInfo, classlike_info::Variance, code_location::HPos,
    codebase_info::CodebaseInfo, t_atomic::TAtomic, taint::string_to_source_types,
    type_definition_info::TypeDefinitionInfo, type_resolution::TypeResolutionContext,
};
use hakana_type::get_mixed_any;
use indexmap::IndexMap;
use oxidized::{
    aast,
    aast_visitor::{visit, AstParams, Node, Visitor},
    ast_defs,
};
use rustc_hash::{FxHashMap, FxHashSet};

mod classlike_scanner;
mod functionlike_scanner;
pub mod simple_type_inferer;
pub mod typehint_resolver;

#[derive(Clone)]
struct Context {
    classlike_name: Option<String>,
    function_name: Option<String>,
    has_yield: bool,
}

struct Scanner<'a> {
    codebase: &'a mut CodebaseInfo,
    file_source: FileSource,
    resolved_names: FxHashMap<usize, String>,
    user_defined: bool,
}

impl<'ast> Visitor<'ast> for Scanner<'_> {
    type Params = AstParams<Context, ()>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params = Self::Params> {
        self
    }

    fn visit_class_(&mut self, c: &mut Context, class: &aast::Class_<(), ()>) -> Result<(), ()> {
        let mut class_name = class.name.1.clone();

        if let Some(resolved_name) = self.resolved_names.get(&class.name.0.start_offset()) {
            class_name = resolved_name.clone();
        }

        if class_name.starts_with("\\") {
            class_name = class_name[1..].to_string();
        }

        self.codebase
            .classlikes_in_files
            .entry((*self.file_source.file_path).clone())
            .or_insert_with(FxHashSet::default)
            .insert(class_name.clone());

        classlike_scanner::scan(
            self.codebase,
            &self.resolved_names,
            &class_name,
            class,
            &self.file_source,
            self.user_defined,
        );

        class.recurse(
            &mut Context {
                classlike_name: Some(class_name.clone()),
                function_name: None,
                ..*c
            },
            self,
        )
    }

    fn visit_gconst(&mut self, c: &mut Context, gc: &aast::Gconst<(), ()>) -> Result<(), ()> {
        let mut name = gc.name.1.clone();

        if let Some(resolved_name) = self.resolved_names.get(&gc.name.0.start_offset()) {
            name = resolved_name.clone();
        }

        self.codebase
            .const_files
            .entry((*self.file_source.file_path).clone())
            .or_insert_with(FxHashSet::default)
            .insert(name.clone());

        self.codebase.constant_infos.insert(
            name,
            ConstantInfo {
                pos: Some(HPos::new(&gc.name.0, &self.file_source.file_path)),
                type_pos: if let Some(t) = &gc.type_ {
                    Some(HPos::new(&t.0, &self.file_source.file_path))
                } else {
                    None
                },
                provided_type: if let Some(t) = &gc.type_ {
                    Some(get_type_from_hint(
                        &*t.1,
                        None,
                        &TypeResolutionContext::new(),
                        &self.resolved_names,
                    ))
                } else {
                    None
                },
                inferred_type: simple_type_inferer::infer(
                    self.codebase,
                    &mut FxHashMap::default(),
                    &gc.value,
                    &self.resolved_names,
                ),
                unresolved_value: None,
                is_abstract: false,
            },
        );

        gc.recurse(c, self)
    }

    fn visit_func_body(&mut self, c: &mut Context, p: &aast::FuncBody<(), ()>) -> Result<(), ()> {
        if self.file_source.file_path.starts_with("hsl_embedded_") {
            Result::Ok(())
        } else {
            p.recurse(c, self)
        }
    }

    fn visit_typedef(
        &mut self,
        c: &mut Context,
        typedef: &aast::Typedef<(), ()>,
    ) -> Result<(), ()> {
        let mut type_name = typedef.name.1.clone();

        if let Some(resolved_name) = self.resolved_names.get(&typedef.name.0.start_offset()) {
            type_name = resolved_name.clone();
        }

        self.codebase
            .typedefs_in_files
            .entry((*self.file_source.file_path).clone())
            .or_insert_with(FxHashSet::default)
            .insert(type_name.clone());

        let mut template_type_map = IndexMap::new();

        let mut generic_variance = FxHashMap::default();

        for (i, param) in typedef.tparams.iter().enumerate() {
            let constraint = param.constraints.first();

            let constraint_type = if let Some(k) = constraint {
                get_type_from_hint(
                    &k.1 .1,
                    None,
                    &TypeResolutionContext::new(),
                    &self.resolved_names,
                )
            } else {
                get_mixed_any()
            };

            let mut h = FxHashMap::default();
            h.insert(
                "typedef-".to_string() + &type_name,
                Arc::new(constraint_type.clone()),
            );

            match param.variance {
                ast_defs::Variance::Covariant => {
                    generic_variance.insert(i, Variance::Covariant);
                }
                ast_defs::Variance::Contravariant => {
                    generic_variance.insert(i, Variance::Contravariant);
                }
                ast_defs::Variance::Invariant => {
                    generic_variance.insert(i, Variance::Invariant);
                }
            }

            template_type_map.insert(param.name.1.clone(), h);
        }

        let mut type_definition = TypeDefinitionInfo {
            newtype_file: if typedef.vis.is_opaque() {
                Some(self.file_source.file_path.clone())
            } else {
                None
            },
            as_type: if let Some(as_hint) = &typedef.constraint {
                Some(get_type_from_hint(
                    &as_hint.1,
                    None,
                    &TypeResolutionContext {
                        template_type_map: template_type_map.clone(),
                        template_supers: FxHashMap::default(),
                    },
                    &self.resolved_names,
                ))
            } else {
                None
            },
            actual_type: get_type_from_hint(
                &typedef.kind.1,
                None,
                &TypeResolutionContext {
                    template_type_map: template_type_map.clone(),
                    template_supers: FxHashMap::default(),
                },
                &self.resolved_names,
            ),
            template_types: template_type_map,
            generic_variance,
            shape_field_taints: None,
            is_literal_string: typedef.user_attributes.iter().any(|user_attribute| {
                let name = if let Some(name) = self
                    .resolved_names
                    .get(&user_attribute.name.0.start_offset())
                {
                    name.clone()
                } else {
                    user_attribute.name.1.clone()
                };

                name == "Hakana\\SpecialTypes\\LiteralString"
            }),
        };

        let shape_source_attribute = typedef
            .user_attributes
            .iter()
            .filter(|user_attribute| {
                let name = if let Some(name) = self
                    .resolved_names
                    .get(&user_attribute.name.0.start_offset())
                {
                    name.clone()
                } else {
                    user_attribute.name.1.clone()
                };

                name == "Hakana\\SecurityAnalysis\\ShapeSource"
            })
            .next();

        if let Some(shape_source_attribute) = shape_source_attribute {
            let mut shape_sinks = FxHashMap::default();

            let attribute_param_expr = &shape_source_attribute.params[0];

            let attribute_param_type = simple_type_inferer::infer(
                &self.codebase,
                &mut FxHashMap::default(),
                attribute_param_expr,
                &self.resolved_names,
            );

            if let Some(attribute_param_type) = attribute_param_type {
                let atomic_param_attribute = attribute_param_type.get_single();

                if let TAtomic::TDict {
                    known_items: Some(attribute_known_items),
                    ..
                } = atomic_param_attribute
                {
                    for (name, (_, item_type)) in attribute_known_items {
                        let mut sink_types = FxHashSet::default();

                        if let Some(str) = item_type.get_single_literal_string_value() {
                            sink_types.extend(string_to_source_types(str));
                        }

                        shape_sinks.insert(name.clone(), sink_types);
                    }
                }
            }

            type_definition.shape_field_taints = Some(shape_sinks);
        }

        self.codebase.symbols.add_typedef_name(type_name.clone());
        self.codebase
            .type_definitions
            .insert(type_name.clone(), type_definition);

        typedef.recurse(c, self)
    }

    fn visit_method_(&mut self, c: &mut Context, m: &aast::Method_<(), ()>) -> Result<(), ()> {
        let (method_name, mut functionlike_storage) = functionlike_scanner::scan_method(
            self.codebase,
            &self.resolved_names,
            m,
            c,
            &self.file_source.comments,
            &self.file_source,
        );

        let result = m.recurse(c, self);

        if c.has_yield {
            functionlike_storage.has_yield = true;
        }

        self.codebase
            .classlike_infos
            .get_mut(c.classlike_name.as_ref().unwrap())
            .unwrap()
            .methods
            .insert(method_name, functionlike_storage);

        c.has_yield = false;

        result
    }

    fn visit_fun_(&mut self, c: &mut Context, f: &aast::Fun_<(), ()>) -> Result<(), ()> {
        let resolved_name = self.resolved_names.get(&f.name.0.start_offset());

        let mut name = match resolved_name {
            Some(resolved_name) => resolved_name.clone(),
            None => f.name.1.clone(),
        };

        if name.starts_with("\\") {
            name = name[1..].to_string();
        }

        let is_anonymous = name.contains(";");

        if is_anonymous {
            name = format!("{}:{}", f.name.0.filename(), f.name.0.start_offset());
        }

        let parent_function_storage = if let Some(parent_function_id) = &c.function_name {
            self.codebase.functionlike_infos.get(parent_function_id)
        } else {
            None
        };

        let mut type_resolution_context = TypeResolutionContext {
            template_type_map: if let Some(parent_function_storage) = parent_function_storage {
                parent_function_storage.template_types.clone()
            } else {
                IndexMap::new()
            },
            template_supers: FxHashMap::default(),
        };

        let mut functionlike_storage = functionlike_scanner::get_functionlike(
            &self.codebase,
            name.clone(),
            &f.span,
            &f.name.0,
            &f.tparams,
            &f.params,
            &f.ret,
            &f.fun_kind,
            &f.user_attributes,
            &f.ctxs,
            &mut type_resolution_context,
            None,
            &self.resolved_names,
            name.clone(),
            &self.file_source.comments,
            &self.file_source,
        );

        functionlike_storage.user_defined = self.user_defined && !is_anonymous;

        functionlike_storage.type_resolution_context = Some(type_resolution_context);

        self.codebase
            .functionlike_infos
            .insert(name.clone(), functionlike_storage);

        self.codebase
            .functions_in_files
            .entry((*self.file_source.file_path).clone())
            .or_insert_with(FxHashSet::default)
            .insert(name.clone());

        if !is_anonymous {
            c.function_name = Some(name);
        }

        let result = f.recurse(c, self);

        c.has_yield = false;

        if !is_anonymous {
            c.function_name = None;
        }

        result
    }

    fn visit_expr(&mut self, c: &mut Context, p: &aast::Expr<(), ()>) -> Result<(), ()> {
        let result = p.recurse(c, self);

        match &p.2 {
            aast::Expr_::Yield(_) => {
                c.has_yield = true;
            }
            _ => (),
        }

        result
    }
}

pub fn collect_info_for_aast(
    program: &aast::Program<(), ()>,
    resolved_names: FxHashMap<usize, String>,
    codebase: &mut CodebaseInfo,
    file_source: FileSource,
    user_defined: bool,
) {
    let mut checker = Scanner {
        codebase,
        file_source,
        resolved_names,
        user_defined,
    };

    let mut context = Context {
        classlike_name: None,
        function_name: None,
        has_yield: false,
    };
    visit(&mut checker, &mut context, program).unwrap();
}
