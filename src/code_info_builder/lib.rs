use std::sync::Arc;

use crate::typehint_resolver::get_type_from_hint;
use hakana_reflection_info::file_info::FileInfo;
use hakana_reflection_info::functionlike_info::FunctionLikeInfo;
use hakana_reflection_info::{
    ast_signature::DefSignatureNode, class_constant_info::ConstantInfo, classlike_info::Variance,
    code_location::HPos, codebase_info::CodebaseInfo, t_atomic::TAtomic,
    taint::string_to_source_types, type_definition_info::TypeDefinitionInfo,
    type_resolution::TypeResolutionContext, StrId,
};
use hakana_reflection_info::{FileSource, ThreadedInterner};
use hakana_type::get_mixed_any;
use indexmap::IndexMap;
use no_pos_hash::position_insensitive_hash;
use oxidized::ast::{FunParam, Tparam, TypeHint};
use oxidized::ast_defs::Id;
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
    classlike_name: Option<StrId>,
    function_name: Option<StrId>,
    method_name: Option<StrId>,
    has_yield: bool,
    uses_position: Option<(usize, usize)>,
    namespace_position: Option<(usize, usize)>,
}

struct Scanner<'a> {
    codebase: &'a mut CodebaseInfo,
    interner: &'a mut ThreadedInterner,
    file_source: FileSource,
    resolved_names: &'a FxHashMap<usize, StrId>,
    all_custom_issues: &'a FxHashSet<String>,
    user_defined: bool,
    closures: FxHashMap<usize, FunctionLikeInfo>,
    ast_nodes: Vec<DefSignatureNode>,
}

impl<'ast> Visitor<'ast> for Scanner<'_> {
    type Params = AstParams<Context, ()>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params = Self::Params> {
        self
    }

    fn visit_def(&mut self, c: &mut Context, p: &aast::Def<(), ()>) -> Result<(), ()> {
        match p {
            aast::Def::Namespace(ns) => {
                if !ns.0 .1.is_empty() {
                    c.namespace_position =
                        Some((ns.0 .0.start_offset() - 10, ns.0 .0.end_offset()));
                }
            }
            aast::Def::NamespaceUse(uses) => {
                for (_, name, alias_name) in uses {
                    let adjusted_start = name.0.to_raw_span().start.beg_of_line() as usize;
                    if let Some(ref mut uses_position) = c.uses_position {
                        uses_position.0 = std::cmp::min(uses_position.0, adjusted_start);
                        uses_position.1 =
                            std::cmp::max(uses_position.1, alias_name.0.end_offset() + 1);
                    } else {
                        c.uses_position = Some((adjusted_start, alias_name.0.end_offset() + 1));
                    }
                }
            }
            _ => {}
        }

        let result = p.recurse(c, self);

        if let aast::Def::Namespace(_) = p {
            c.namespace_position = None;
            c.uses_position = None;
        }

        result
    }

    fn visit_class_(&mut self, c: &mut Context, class: &aast::Class_<(), ()>) -> Result<(), ()> {
        let class_name = self
            .resolved_names
            .get(&class.name.0.start_offset())
            .unwrap()
            .clone();

        classlike_scanner::scan(
            self.codebase,
            self.interner,
            &self.all_custom_issues,
            &self.resolved_names,
            &class_name,
            class,
            &self.file_source,
            self.user_defined,
            &self.file_source.comments,
            c.namespace_position,
            c.uses_position,
            &mut self.ast_nodes,
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
        let name = self
            .resolved_names
            .get(&gc.name.0.start_offset())
            .unwrap()
            .clone();

        self.codebase
            .const_files
            .entry((self.file_source.file_path_actual).clone())
            .or_insert_with(FxHashSet::default)
            .insert(name.clone());

        let definition_location = HPos::new(&gc.name.0, self.file_source.file_path, None);

        self.ast_nodes.push(DefSignatureNode {
            name,
            start_offset: definition_location.start_offset,
            end_offset: definition_location.end_offset,
            start_line: definition_location.start_line,
            end_line: definition_location.end_line,
            children: Vec::new(),
            signature_hash: { position_insensitive_hash(gc) },
            body_hash: None,
        });

        self.codebase.symbols.add_constant_name(name);

        self.codebase.constant_infos.insert(
            name,
            ConstantInfo {
                pos: definition_location,
                type_pos: if let Some(t) = &gc.type_ {
                    Some(HPos::new(&t.0, self.file_source.file_path, None))
                } else {
                    None
                },
                provided_type: if let Some(t) = &gc.type_ {
                    get_type_from_hint(
                        &*t.1,
                        None,
                        &TypeResolutionContext::new(),
                        &self.resolved_names,
                    )
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
        if !self.user_defined {
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
        let type_name = self
            .resolved_names
            .get(&typedef.name.0.start_offset())
            .unwrap()
            .clone();

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
                .unwrap()
            } else {
                get_mixed_any()
            };

            let mut h = FxHashMap::default();
            let type_name_str = self.interner.lookup(type_name).to_string();
            h.insert(
                self.interner
                    .intern("typedef-".to_string() + &type_name_str),
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

        let mut definition_location = HPos::new(&typedef.span, self.file_source.file_path, None);

        if let Some(user_attribute) = typedef.user_attributes.get(0) {
            definition_location.start_line = user_attribute.name.0.line();
            definition_location.start_offset = user_attribute.name.0.start_offset();
        }

        self.ast_nodes.push(DefSignatureNode {
            name: type_name,
            start_offset: definition_location.start_offset,
            end_offset: definition_location.end_offset,
            start_line: definition_location.start_line,
            end_line: definition_location.end_line,
            children: Vec::new(),
            signature_hash: { position_insensitive_hash(typedef) },
            body_hash: None,
        });

        let mut type_definition = TypeDefinitionInfo {
            newtype_file: if typedef.vis.is_opaque() {
                Some(self.file_source.file_path)
            } else {
                None
            },
            as_type: if let Some(as_hint) = &typedef.as_constraint {
                get_type_from_hint(
                    &as_hint.1,
                    None,
                    &TypeResolutionContext {
                        template_type_map: template_type_map.clone(),
                        template_supers: FxHashMap::default(),
                    },
                    &self.resolved_names,
                )
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
            )
            .unwrap(),
            template_types: template_type_map,
            generic_variance,
            shape_field_taints: None,
            is_literal_string: typedef.user_attributes.iter().any(|user_attribute| {
                self.interner.lookup(
                    *self
                        .resolved_names
                        .get(&user_attribute.name.0.start_offset())
                        .unwrap(),
                ) == "Hakana\\SpecialTypes\\LiteralString"
            }),
        };

        let shape_source_attribute = typedef
            .user_attributes
            .iter()
            .filter(|user_attribute| {
                self.interner.lookup(
                    *self
                        .resolved_names
                        .get(&user_attribute.name.0.start_offset())
                        .unwrap(),
                ) == "Hakana\\SecurityAnalysis\\ShapeSource"
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

                        if let Some(str) =
                            item_type.get_single_literal_string_value(&self.codebase.interner)
                        {
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
        let (method_name, functionlike_storage) = functionlike_scanner::scan_method(
            self.codebase,
            self.interner,
            self.all_custom_issues,
            &self.resolved_names,
            m,
            c,
            &self.file_source.comments,
            &self.file_source,
        );

        c.method_name = Some(method_name);

        if let Some(last_current_node) = self.ast_nodes.last_mut() {
            let (signature_hash, body_hash) = get_function_hashes(
                &self.file_source.file_contents,
                &functionlike_storage.def_location,
                &m.name,
                &m.tparams,
                &m.params,
                &m.ret,
                &m.body,
            );
            last_current_node.children.push(DefSignatureNode {
                name: functionlike_storage.name,
                start_offset: functionlike_storage.def_location.start_offset,
                end_offset: functionlike_storage.def_location.end_offset,
                start_line: functionlike_storage.def_location.start_line,
                end_line: functionlike_storage.def_location.end_line,
                signature_hash,
                body_hash: Some(body_hash),
                children: vec![],
            });
        }

        self.codebase
            .classlike_infos
            .get_mut(c.classlike_name.as_ref().unwrap())
            .unwrap()
            .methods
            .insert(method_name, functionlike_storage);

        let result = m.recurse(c, self);

        c.method_name = None;

        if c.has_yield {
            self.codebase
                .classlike_infos
                .get_mut(c.classlike_name.as_ref().unwrap())
                .unwrap()
                .methods
                .get_mut(&method_name)
                .unwrap()
                .has_yield = true;
            c.has_yield = false;
        }

        result
    }

    fn visit_fun_(&mut self, c: &mut Context, f: &aast::Fun_<(), ()>) -> Result<(), ()> {
        let mut name = self
            .resolved_names
            .get(&f.name.0.start_offset())
            .unwrap()
            .clone();

        let is_anonymous = f.name.1.contains(";");

        if is_anonymous {
            let function_id = format!("{}:{}", f.name.0.filename(), f.name.0.start_offset());

            name = self.interner.intern(function_id);
        }

        let parent_function_storage = if is_anonymous {
            if let Some(parent_function_id) = &c.function_name {
                self.codebase.functionlike_infos.get(parent_function_id)
            } else if let (Some(parent_class_id), Some(parent_method_id)) =
                (&c.classlike_name, &c.method_name)
            {
                if let Some(classlike_info) = self.codebase.classlike_infos.get(parent_class_id) {
                    classlike_info.methods.get(parent_method_id)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let mut template_type_map = if let Some(parent_function_storage) = parent_function_storage {
            parent_function_storage.template_types.clone()
        } else {
            IndexMap::new()
        };

        if let Some(classlike_name) = &c.classlike_name {
            template_type_map.extend(
                self.codebase
                    .classlike_infos
                    .get(classlike_name)
                    .unwrap()
                    .template_types
                    .clone(),
            );
        }

        let mut type_resolution_context = TypeResolutionContext {
            template_type_map,
            template_supers: FxHashMap::default(),
        };

        let functionlike_id = self.interner.lookup(name).to_string();

        let mut functionlike_storage = functionlike_scanner::get_functionlike(
            &self.codebase,
            self.interner,
            self.all_custom_issues,
            name.clone(),
            &f.span,
            &f.name.0,
            &f.tparams,
            &f.params,
            &f.ret,
            &f.fun_kind,
            &f.user_attributes,
            &f.ctxs,
            &f.where_constraints,
            &mut type_resolution_context,
            None,
            &self.resolved_names,
            &functionlike_id,
            &self.file_source.comments,
            &self.file_source,
            is_anonymous,
        );

        functionlike_storage.user_defined = self.user_defined && !is_anonymous;

        functionlike_storage.type_resolution_context = Some(type_resolution_context);

        if !is_anonymous {
            let (signature_hash, body_hash) = get_function_hashes(
                &self.file_source.file_contents,
                &functionlike_storage.def_location,
                &f.name,
                &f.tparams,
                &f.params,
                &f.ret,
                &f.body,
            );

            self.ast_nodes.push(DefSignatureNode {
                name,
                start_offset: functionlike_storage.def_location.start_offset,
                end_offset: functionlike_storage.def_location.end_offset,
                start_line: functionlike_storage.def_location.start_line,
                end_line: functionlike_storage.def_location.end_line,
                children: Vec::new(),
                signature_hash,
                body_hash: Some(body_hash),
            });

            self.codebase
                .functionlike_infos
                .insert(name.clone(), functionlike_storage);

            c.function_name = Some(name);
        } else {
            self.closures
                .insert(f.span.start_offset(), functionlike_storage);
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

fn get_function_hashes(
    file_contents: &String,
    def_location: &HPos,
    name: &Id,
    tparams: &Vec<Tparam>,
    params: &Vec<FunParam>,
    ret: &TypeHint,
    body: &aast::FuncBody<(), ()>,
) -> (u64, u64) {
    let body_hash = position_insensitive_hash(body);

    let mut signature_end = name.0.end_offset();

    if let Some(last_tparam) = tparams.last() {
        signature_end = last_tparam.name.0.end_offset();

        if let Some((_, last_tparam_constraint)) = last_tparam.constraints.last() {
            signature_end = last_tparam_constraint.0.end_offset();
        }
    }

    if let Some(last_param) = params.last() {
        if let Some(expr) = &last_param.expr {
            signature_end = expr.1.end_offset();
        }

        if let Some(last_hint) = &last_param.type_hint.1 {
            signature_end = last_hint.0.end_offset();
        }
    }

    if let Some(ret_hint) = &ret.1 {
        signature_end = ret_hint.0.end_offset();
    }

    let signature_hash = xxhash_rust::xxh3::xxh3_64(
        file_contents[def_location.start_offset..signature_end].as_bytes(),
    );

    (signature_hash, body_hash)
}

pub fn collect_info_for_aast(
    program: &aast::Program<(), ()>,
    resolved_names: &FxHashMap<usize, StrId>,
    interner: &mut ThreadedInterner,
    codebase: &mut CodebaseInfo,
    all_custom_issues: &FxHashSet<String>,
    file_source: FileSource,
    user_defined: bool,
) {
    let file_path_id = file_source.file_path;

    let mut checker = Scanner {
        codebase,
        interner,
        file_source,
        resolved_names,
        user_defined,
        all_custom_issues,
        closures: FxHashMap::default(),
        ast_nodes: Vec::new(),
    };

    let mut context = Context {
        classlike_name: None,
        function_name: None,
        method_name: None,
        has_yield: false,
        uses_position: None,
        namespace_position: None,
    };
    visit(&mut checker, &mut context, program).unwrap();

    checker.codebase.files.insert(
        file_path_id,
        FileInfo {
            closure_infos: checker.closures,
            ast_nodes: checker.ast_nodes,
        },
    );
}
