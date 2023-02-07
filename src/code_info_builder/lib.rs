use std::hash::Hash;
use std::sync::Arc;

use crate::typehint_resolver::get_type_from_hint;
use hakana_aast_helper::Uses;
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
use no_pos_hash::{position_insensitive_hash, Hasher};
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
    member_name: Option<StrId>,
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
    uses: Uses,
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
            &self.uses,
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

        let uses_hash = get_uses_hash(self.uses.symbol_uses.get(&name).unwrap_or(&vec![]));

        self.ast_nodes.push(DefSignatureNode {
            name,
            start_offset: definition_location.start_offset,
            end_offset: definition_location.end_offset,
            start_line: definition_location.start_line,
            end_line: definition_location.end_line,
            children: Vec::new(),
            signature_hash: { position_insensitive_hash(gc).wrapping_add(uses_hash) },
            body_hash: None,
            is_function: false,
            is_constant: true,
        });

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

        let mut type_context = TypeResolutionContext::new();

        for type_param_node in typedef.tparams.iter() {
            let param_name = self
                .resolved_names
                .get(&type_param_node.name.0.start_offset())
                .unwrap();
            type_context.template_type_map.insert(
                *param_name,
                FxHashMap::from_iter([(type_name.clone(), Arc::new(get_mixed_any()))]),
            );
        }

        for (i, param) in typedef.tparams.iter().enumerate() {
            let constraint = param.constraints.first();

            let constraint_type = if let Some(k) = constraint {
                get_type_from_hint(&k.1 .1, None, &type_context, &self.resolved_names).unwrap()
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

            let param_name = self
                .resolved_names
                .get(&param.name.0.start_offset())
                .unwrap();

            template_type_map.insert(*param_name, h);
        }

        let mut definition_location = HPos::new(&typedef.span, self.file_source.file_path, None);

        if let Some(user_attribute) = typedef.user_attributes.get(0) {
            definition_location.start_line = user_attribute.name.0.line();
            definition_location.start_offset = user_attribute.name.0.start_offset();
        }

        let uses_hash = get_uses_hash(self.uses.symbol_uses.get(&type_name).unwrap_or(&vec![]));

        self.ast_nodes.push(DefSignatureNode {
            name: type_name,
            start_offset: definition_location.start_offset,
            end_offset: definition_location.end_offset,
            start_line: definition_location.start_line,
            end_line: definition_location.end_line,
            children: Vec::new(),
            signature_hash: { position_insensitive_hash(typedef).wrapping_add(uses_hash) },
            body_hash: None,
            is_function: false,
            is_constant: false,
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
            location: definition_location,
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
            let mut shape_sources = FxHashMap::default();

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
                        let mut source_types = FxHashSet::default();

                        if let Some(str) =
                            item_type.get_single_literal_string_value(&self.codebase.interner)
                        {
                            if let Some(source_type) = string_to_source_types(str) {
                                source_types.insert(source_type);
                            }
                        }

                        shape_sources.insert(
                            name.clone(),
                            (
                                HPos::new(
                                    shape_source_attribute.name.pos(),
                                    self.file_source.file_path,
                                    None,
                                ),
                                source_types,
                            ),
                        );
                    }
                }
            }

            type_definition.shape_field_taints = Some(shape_sources);
        }

        self.codebase.symbols.add_typedef_name(type_name.clone());
        self.codebase
            .type_definitions
            .insert(type_name.clone(), type_definition);

        typedef.recurse(c, self)
    }

    fn visit_class_const(
        &mut self,
        c: &mut Context,
        m: &aast::ClassConst<(), ()>,
    ) -> Result<(), ()> {
        let member_name = self.interner.intern(m.id.1.clone());

        c.member_name = Some(member_name);

        let result = m.recurse(c, self);

        c.member_name = None;

        result
    }

    fn visit_class_typeconst_def(
        &mut self,
        c: &mut Context,
        m: &aast::ClassTypeconstDef<(), ()>,
    ) -> Result<(), ()> {
        let member_name = self.interner.intern(m.name.1.clone());

        c.member_name = Some(member_name);

        let result = m.recurse(c, self);

        c.member_name = None;

        result
    }

    fn visit_class_var(&mut self, c: &mut Context, m: &aast::ClassVar<(), ()>) -> Result<(), ()> {
        let member_name = self.interner.intern(m.id.1.clone());

        c.member_name = Some(member_name);

        let result = m.recurse(c, self);

        c.member_name = None;

        result
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

        c.member_name = Some(method_name);

        if let Some(last_current_node) = self.ast_nodes.last_mut() {
            let (signature_hash, body_hash) = get_function_hashes(
                &self.file_source.file_contents,
                &functionlike_storage.def_location,
                &m.name,
                &m.tparams,
                &m.params,
                &m.ret,
                &self
                    .uses
                    .symbol_member_uses
                    .get(&(c.classlike_name.unwrap(), c.member_name.unwrap()))
                    .unwrap_or(&vec![]),
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
                is_function: true,
                is_constant: false,
            });
        }

        self.codebase
            .classlike_infos
            .get_mut(c.classlike_name.as_ref().unwrap())
            .unwrap()
            .methods
            .insert(method_name, functionlike_storage);

        let result = m.recurse(c, self);

        c.member_name = None;

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

    fn visit_fun_def(&mut self, c: &mut Context, f: &aast::FunDef<(), ()>) -> Result<(), ()> {
        let name = self
            .resolved_names
            .get(&f.name.0.start_offset())
            .unwrap()
            .clone();

        let functionlike_id = self.interner.lookup(name).to_string();

        let functionlike_storage =
            self.visit_function(false, c, name, &f.fun, Some(&f.name.0), functionlike_id);

        let (signature_hash, body_hash) = get_function_hashes(
            &self.file_source.file_contents,
            &functionlike_storage.def_location,
            &f.name,
            &f.fun.tparams,
            &f.fun.params,
            &f.fun.ret,
            &self.uses.symbol_uses.get(&name).unwrap_or(&vec![]),
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
            is_function: true,
            is_constant: false,
        });

        self.codebase
            .functionlike_infos
            .insert(name.clone(), functionlike_storage);

        c.function_name = Some(name);

        let result = f.recurse(c, self);

        c.has_yield = false;

        c.function_name = None;

        result
    }

    fn visit_expr(&mut self, c: &mut Context, p: &aast::Expr<(), ()>) -> Result<(), ()> {
        let result = p.recurse(c, self);

        let mut fun = None;
        match &p.2 {
            aast::Expr_::Yield(_) => {
                c.has_yield = true;
            }
            aast::Expr_::Lfun(f) => {
                fun = Some(&f.0);
            }
            aast::Expr_::Efun(f) => {
                fun = Some(&f.fun);
            }
            _ => (),
        }

        if let Some(fun) = fun {
            let function_id = format!("{}:{}", fun.span.filename(), fun.span.start_offset());

            let name = self.interner.intern(function_id);

            let functionlike_id = self.interner.lookup(name).to_string();

            let functionlike_storage =
                self.visit_function(true, c, name, fun, None, functionlike_id);

            self.closures
                .insert(fun.span.start_offset(), functionlike_storage);
        }

        result
    }
}

impl<'a> Scanner<'a> {
    fn visit_function(
        &mut self,
        is_anonymous: bool,
        c: &mut Context,
        name: StrId,
        fun: &aast::Fun_<(), ()>,
        name_pos: Option<&oxidized::tast::Pos>,
        functionlike_id: String,
    ) -> FunctionLikeInfo {
        let parent_function_storage = if is_anonymous {
            if let Some(parent_function_id) = &c.function_name {
                self.codebase.functionlike_infos.get(parent_function_id)
            } else if let (Some(parent_class_id), Some(parent_method_id)) =
                (&c.classlike_name, &c.member_name)
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

        let mut functionlike_storage = functionlike_scanner::get_functionlike(
            &self.codebase,
            self.interner,
            self.all_custom_issues,
            name.clone(),
            &fun.span,
            name_pos,
            &fun.tparams,
            &fun.params,
            &fun.body.fb_ast,
            &fun.ret,
            &fun.fun_kind,
            &fun.user_attributes,
            &fun.ctxs,
            &fun.where_constraints,
            &mut type_resolution_context,
            None,
            &self.resolved_names,
            &functionlike_id,
            &self.file_source.comments,
            &self.file_source,
            is_anonymous,
        );

        functionlike_storage.is_production_code = self.file_source.is_production_code;

        functionlike_storage.user_defined = self.user_defined && !is_anonymous;
        functionlike_storage.type_resolution_context = Some(type_resolution_context);
        functionlike_storage
    }
}

fn get_uses_hash(uses: &Vec<(StrId, StrId)>) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    uses.hash(&mut hasher);
    hasher.finish()
}

fn get_function_hashes(
    file_contents: &String,
    def_location: &HPos,
    name: &Id,
    tparams: &Vec<Tparam>,
    params: &Vec<FunParam>,
    ret: &TypeHint,
    uses: &Vec<(StrId, StrId)>,
) -> (u64, u64) {
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

    (
        signature_hash,
        xxhash_rust::xxh3::xxh3_64(
            file_contents[signature_end..def_location.end_offset].as_bytes(),
        )
        .wrapping_add(get_uses_hash(uses)),
    )
}

pub fn collect_info_for_aast(
    program: &aast::Program<(), ()>,
    resolved_names: &FxHashMap<usize, StrId>,
    interner: &mut ThreadedInterner,
    codebase: &mut CodebaseInfo,
    all_custom_issues: &FxHashSet<String>,
    file_source: FileSource,
    user_defined: bool,
    uses: Uses,
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
        uses,
    };

    let mut context = Context {
        classlike_name: None,
        function_name: None,
        member_name: None,
        has_yield: false,
        uses_position: None,
        namespace_position: None,
    };
    visit(&mut checker, &mut context, program).unwrap();

    if user_defined {
        checker.codebase.files.insert(
            file_path_id,
            FileInfo {
                closure_infos: checker.closures,
                ast_nodes: checker.ast_nodes,
            },
        );
    }
}
