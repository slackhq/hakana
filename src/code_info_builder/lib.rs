use std::hash::Hash;
use std::sync::Arc;

use crate::typehint_resolver::get_type_from_hint;
use hakana_aast_helper::Uses;
use hakana_code_info::attribute_info::AttributeInfo;
use hakana_code_info::file_info::{FileInfo, ParserError};
use hakana_code_info::functionlike_info::FunctionLikeInfo;
use hakana_code_info::issue::IssueKind;
use hakana_code_info::t_atomic::TDict;
use hakana_code_info::t_union::TUnion;
use hakana_code_info::ttype::{get_bool, get_int, get_mixed_any, get_string};
use hakana_code_info::{
    ast_signature::DefSignatureNode, class_constant_info::ConstantInfo, classlike_info::Variance,
    code_location::HPos, codebase_info::CodebaseInfo, t_atomic::TAtomic,
    taint::string_to_source_types, type_definition_info::TypeDefinitionInfo,
    type_resolution::TypeResolutionContext,
};
use hakana_code_info::{FileSource, GenericParent};
use hakana_str::{StrId, ThreadedInterner};
use no_pos_hash::{position_insensitive_hash, Hasher};
use oxidized::ast::{FunParam, Tparam, TypeHint};
use oxidized::ast_defs::Id;
use oxidized::tast;
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
    has_throw: bool,
    has_asio_join: bool,
    has_static_field_access: bool,
    uses_position: Option<(usize, usize)>,
    namespace_position: Option<(usize, usize)>,
}

struct Scanner<'a> {
    codebase: &'a mut CodebaseInfo,
    interner: &'a mut ThreadedInterner,
    file_source: FileSource<'a>,
    resolved_names: &'a FxHashMap<u32, StrId>,
    all_custom_issues: &'a FxHashSet<String>,
    user_defined: bool,
    closure_refs: Vec<u32>,
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
        let class_name = *self
            .resolved_names
            .get(&(class.name.0.start_offset() as u32))
            .unwrap();

        classlike_scanner::scan(
            self.codebase,
            self.interner,
            self.all_custom_issues,
            self.resolved_names,
            &class_name,
            class,
            &self.file_source,
            self.user_defined,
            self.file_source.comments,
            c.namespace_position,
            c.uses_position,
            &mut self.ast_nodes,
            &self.uses,
        );

        class.recurse(
            &mut Context {
                classlike_name: Some(class_name),
                function_name: None,
                ..*c
            },
            self,
        )
    }

    fn visit_gconst(&mut self, c: &mut Context, gc: &aast::Gconst<(), ()>) -> Result<(), ()> {
        let name = *self
            .resolved_names
            .get(&(gc.name.0.start_offset() as u32))
            .unwrap();

        self.codebase
            .const_files
            .entry((self.file_source.file_path_actual).clone())
            .or_default()
            .insert(name);

        let definition_location = HPos::new(&gc.name.0, self.file_source.file_path);

        let uses_hash = get_uses_hash(self.uses.symbol_uses.get(&name).unwrap_or(&vec![]));

        self.ast_nodes.push(DefSignatureNode {
            name,
            start_offset: definition_location.start_offset,
            end_offset: definition_location.end_offset,
            start_line: definition_location.start_line,
            end_line: definition_location.end_line,
            start_colum: definition_location.start_column,
            end_column: definition_location.end_column,
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
                    Some(HPos::new(&t.0, self.file_source.file_path))
                } else {
                    None
                },
                provided_type: if let Some(t) = &gc.type_ {
                    get_type_from_hint(
                        &t.1,
                        None,
                        &TypeResolutionContext::new(),
                        self.resolved_names,
                        self.file_source.file_path,
                        t.0.start_offset() as u32,
                    )
                } else {
                    None
                },
                inferred_type: simple_type_inferer::infer(&gc.value, self.resolved_names),
                unresolved_value: None,
                is_abstract: false,
                allow_non_exclusive_enum_values: false,
                suppressed_issues: vec![],
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
        let type_name = *self
            .resolved_names
            .get(&(typedef.name.0.start_offset() as u32))
            .unwrap();

        let mut template_type_map = vec![];

        let mut generic_variance = FxHashMap::default();

        let mut type_context = TypeResolutionContext::new();

        for type_param_node in typedef.tparams.iter() {
            let param_name = self
                .resolved_names
                .get(&(type_param_node.name.0.start_offset() as u32))
                .unwrap();
            type_context.template_type_map.push((
                *param_name,
                vec![(
                    GenericParent::TypeDefinition(type_name),
                    Arc::new(get_mixed_any()),
                )],
            ));
        }

        for (i, param) in typedef.tparams.iter().enumerate() {
            let constraint = param.constraints.first();

            let constraint_type = if let Some(k) = constraint {
                get_type_from_hint(
                    &k.1 .1,
                    None,
                    &type_context,
                    self.resolved_names,
                    self.file_source.file_path,
                    k.1 .0.start_offset() as u32,
                )
                .unwrap()
            } else {
                get_mixed_any()
            };

            let h = vec![(
                GenericParent::TypeDefinition(type_name),
                Arc::new(constraint_type.clone()),
            )];

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
                .get(&(param.name.0.start_offset() as u32))
                .unwrap();

            template_type_map.push((*param_name, h));
        }

        let mut definition_location = HPos::new(&typedef.span, self.file_source.file_path);

        if let Some(user_attribute) = typedef.user_attributes.first() {
            definition_location.start_line = user_attribute.name.0.line() as u32;
            definition_location.start_offset = user_attribute.name.0.start_offset() as u32;
        }

        let uses_hash = get_uses_hash(self.uses.symbol_uses.get(&type_name).unwrap_or(&vec![]));

        self.ast_nodes.push(DefSignatureNode {
            name: type_name,
            start_offset: definition_location.start_offset,
            end_offset: definition_location.end_offset,
            start_line: definition_location.start_line,
            end_line: definition_location.end_line,
            start_colum: definition_location.start_column,
            end_column: definition_location.end_column,
            children: Vec::new(),
            signature_hash: { position_insensitive_hash(typedef).wrapping_add(uses_hash) },
            body_hash: None,
            is_function: false,
            is_constant: false,
        });

        let mut is_literal_string = false;
        let mut is_codegen = false;

        let mut shape_source_attribute = None;

        let mut attributes = vec![];

        for user_attribute in &typedef.user_attributes {
            let attribute_name = self
                .resolved_names
                .get(&(user_attribute.name.0.start_offset() as u32))
                .unwrap();

            attributes.push(AttributeInfo {
                name: *attribute_name,
            });

            match *attribute_name {
                StrId::HAKANA_SECURITY_ANALYSIS_SHAPE_SOURCE => {
                    shape_source_attribute = Some(user_attribute);
                    break;
                }
                StrId::HAKANA_SPECIAL_TYPES_LITERAL_STRING => {
                    is_literal_string = true;
                }
                StrId::CODEGEN => {
                    is_codegen = true;
                }
                _ => {}
            }
        }

        let source_file;
        let mut actual_type;

        match &typedef.assignment {
            tast::TypedefAssignment::SimpleTypeDef(typedef_innser) => {
                if typedef_innser.vis.is_opaque() {
                    source_file = Some(self.file_source.file_path);
                } else {
                    source_file = None;
                }

                actual_type = get_type_from_hint(
                    &typedef.runtime_type.1,
                    None,
                    &TypeResolutionContext {
                        template_type_map: template_type_map.clone(),
                        template_supers: vec![],
                    },
                    self.resolved_names,
                    self.file_source.file_path,
                    typedef.runtime_type.0.start_offset() as u32,
                )
                .unwrap();
            }
            tast::TypedefAssignment::CaseType(first_variant, other_variants) => {
                source_file = None;

                actual_type = get_type_from_hint(
                    &first_variant.hint.1,
                    None,
                    &TypeResolutionContext {
                        template_type_map: template_type_map.clone(),
                        template_supers: vec![],
                    },
                    self.resolved_names,
                    self.file_source.file_path,
                    first_variant.hint.0.start_offset() as u32,
                )
                .unwrap();

                for other_variant in other_variants {
                    actual_type.types.extend(
                        get_type_from_hint(
                            &other_variant.hint.1,
                            None,
                            &TypeResolutionContext {
                                template_type_map: template_type_map.clone(),
                                template_supers: vec![],
                            },
                            self.resolved_names,
                            self.file_source.file_path,
                            other_variant.hint.0.start_offset() as u32,
                        )
                        .unwrap()
                        .types,
                    );
                }
            }
        }

        let mut type_definition = TypeDefinitionInfo {
            newtype_file: source_file,
            as_type: if let Some(as_hint) = &typedef.as_constraint {
                get_type_from_hint(
                    &as_hint.1,
                    None,
                    &TypeResolutionContext {
                        template_type_map: template_type_map.clone(),
                        template_supers: vec![],
                    },
                    self.resolved_names,
                    self.file_source.file_path,
                    as_hint.0.start_offset() as u32,
                )
            } else {
                None
            },
            actual_type,
            template_types: template_type_map,
            generic_variance,
            shape_field_taints: None,
            is_literal_string,
            generated: is_codegen,
            location: definition_location,
            user_defined: self.user_defined,
            attributes,
        };

        if let Some(shape_source_attribute) = shape_source_attribute {
            let mut shape_sources = FxHashMap::default();

            let attribute_param_expr = &shape_source_attribute.params[0];

            let attribute_param_type =
                simple_type_inferer::infer(attribute_param_expr, self.resolved_names);

            if let Some(atomic_param_attribute) = attribute_param_type {
                if let TAtomic::TDict(TDict {
                    known_items: Some(attribute_known_items),
                    ..
                }) = atomic_param_attribute
                {
                    for (name, (_, item_type)) in attribute_known_items {
                        let mut source_types = vec![];

                        if let Some(str) = item_type.get_single_literal_string_value() {
                            if let Some(source_type) = string_to_source_types(str) {
                                source_types.push(source_type);
                            }
                        }

                        shape_sources.insert(
                            name.clone(),
                            (
                                HPos::new(
                                    shape_source_attribute.name.pos(),
                                    self.file_source.file_path,
                                ),
                                source_types,
                            ),
                        );
                    }
                }
            }

            type_definition.shape_field_taints = Some(shape_sources);
        }

        self.codebase
            .symbols
            .add_typedef_name(type_name, source_file.is_some());
        self.codebase
            .type_definitions
            .insert(type_name, type_definition);

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
        let method_name = self.interner.intern(m.name.1.clone());

        c.member_name = Some(method_name);

        let result = m.recurse(c, self);

        c.member_name = None;

        let classlike_name = *c.classlike_name.as_ref().unwrap();

        let Some(functionlike_storage) = self
            .codebase
            .functionlike_infos
            .get_mut(&(classlike_name, method_name))
        else {
            println!(
                "Could not load storage for {}::{}",
                self.interner.lookup(classlike_name),
                m.name.1
            );
            panic!()
        };

        if classlike_name == StrId::REFLECTION_CLASS || classlike_name == StrId::REFLECTION_FUNCTION
        {
            functionlike_storage.has_throw = true;
        }

        if c.has_yield {
            functionlike_storage.has_yield = true;
            c.has_yield = false;
        }

        if c.has_throw {
            functionlike_storage.has_throw = true;
            c.has_throw = false;
        }

        if c.has_asio_join {
            functionlike_storage.has_asio_join = true;
            c.has_asio_join = false;
        }

        if !c.has_static_field_access && m.static_ {
            functionlike_storage.specialize_call = true;
            c.has_static_field_access = false;
        }

        result
    }

    fn visit_fun_def(&mut self, c: &mut Context, f: &aast::FunDef<(), ()>) -> Result<(), ()> {
        let name = *self
            .resolved_names
            .get(&(f.name.0.start_offset() as u32))
            .unwrap();

        let mut functionlike_storage = self.visit_function(
            c,
            Some(name),
            &f.fun,
            &f.tparams,
            &f.where_constraints,
            Some(&f.name.0),
        );

        let (signature_hash, body_hash) = get_function_hashes(
            &self.file_source.file_contents,
            &functionlike_storage.def_location,
            &f.name,
            &f.tparams,
            &f.fun.params,
            &functionlike_storage.attributes,
            functionlike_storage
                .suppressed_issues
                .iter()
                .map(|k| &k.0)
                .collect(),
            &f.fun.ret,
            self.uses.symbol_uses.get(&name).unwrap_or(&vec![]),
        );

        self.ast_nodes.push(DefSignatureNode {
            name,
            start_offset: functionlike_storage.def_location.start_offset,
            end_offset: functionlike_storage.def_location.end_offset,
            start_line: functionlike_storage.def_location.start_line,
            end_line: functionlike_storage.def_location.end_line,
            start_colum: functionlike_storage.def_location.start_column,
            end_column: functionlike_storage.def_location.end_column,
            children: Vec::new(),
            signature_hash,
            body_hash: Some(body_hash),
            is_function: true,
            is_constant: false,
        });

        c.function_name = Some(name);

        let result = f.recurse(c, self);

        if c.has_yield {
            functionlike_storage.has_yield = true;
        }

        if c.has_throw {
            functionlike_storage.has_throw = true;
        }

        if c.has_asio_join {
            functionlike_storage.has_asio_join = true;
        }

        self.codebase
            .functionlike_infos
            .insert((name, StrId::EMPTY), functionlike_storage);

        c.has_yield = false;
        c.has_static_field_access = false;
        c.has_asio_join = false;

        c.function_name = None;

        result
    }

    fn visit_stmt_(&mut self, c: &mut Context, p: &aast::Stmt_<(), ()>) -> Result<(), ()> {
        let result = p.recurse(c, self);

        if let aast::Stmt_::Throw(..) = &p {
            c.has_throw = true;
        }

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
            aast::Expr_::ClassGet(boxed) => {
                if let ast_defs::PropOrMethod::IsProp = &boxed.2 {
                    c.has_static_field_access = true;
                }
            }
            aast::Expr_::ObjGet(boxed) => {
                if let aast::Expr_::Id(id) = &boxed.1 .2 {
                    self.interner.intern(id.1.clone());
                }
            }
            aast::Expr_::Call(boxed) => {
                if let aast::Expr_::Id(boxed_id) = &boxed.func.2 {
                    if let Some(&StrId::ASIO_JOIN) =
                        self.resolved_names.get(&(boxed_id.0.start_offset() as u32))
                    {
                        c.has_asio_join = true;
                    }
                }
            }
            _ => (),
        }

        if let Some(fun) = fun {
            let had_yield = c.has_yield;
            let had_throw = c.has_throw;
            let had_asio_join = c.has_asio_join;

            let mut functionlike_storage = self.visit_function(c, None, fun, &[], &vec![], None);

            if c.has_yield {
                functionlike_storage.has_yield = true;
            }

            if c.has_throw {
                functionlike_storage.has_throw = true;
            }

            if c.has_asio_join {
                functionlike_storage.has_asio_join = true;
            }

            c.has_yield = had_yield;
            c.has_throw = had_throw;
            c.has_asio_join = had_asio_join;

            self.codebase.functionlike_infos.insert(
                (
                    self.file_source.file_path.0,
                    StrId(fun.span.start_offset() as u32),
                ),
                functionlike_storage,
            );

            self.closure_refs.push(fun.span.start_offset() as u32);
        }

        result
    }
}

impl<'a> Scanner<'a> {
    fn visit_function(
        &mut self,
        c: &mut Context,
        name: Option<StrId>,
        fun: &aast::Fun_<(), ()>,
        tparams: &[aast::Tparam<(), ()>],
        where_constraints: &Vec<aast::WhereConstraintHint>,
        name_pos: Option<&oxidized::tast::Pos>,
    ) -> FunctionLikeInfo {
        let parent_function_storage = if name.is_none() {
            if let Some(parent_function_id) = &c.function_name {
                self.codebase
                    .functionlike_infos
                    .get(&(*parent_function_id, StrId::EMPTY))
            } else if let (Some(parent_class_id), Some(parent_method_id)) =
                (&c.classlike_name, &c.member_name)
            {
                self.codebase
                    .functionlike_infos
                    .get(&(*parent_class_id, *parent_method_id))
            } else {
                None
            }
        } else {
            None
        };

        let mut template_type_map = if let Some(parent_function_storage) = parent_function_storage {
            parent_function_storage.template_types.clone()
        } else {
            vec![]
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
            template_supers: vec![],
        };

        let mut functionlike_storage = functionlike_scanner::get_functionlike(
            self.interner,
            self.all_custom_issues,
            name,
            &fun.span,
            name_pos,
            tparams,
            &fun.params,
            &fun.body.fb_ast.0,
            &fun.ret,
            &fun.fun_kind,
            &fun.user_attributes.0,
            &fun.ctxs,
            where_constraints,
            &mut type_resolution_context,
            None,
            self.resolved_names,
            self.file_source.comments,
            &self.file_source,
            self.user_defined,
        );

        functionlike_storage.is_production_code &= self.file_source.is_production_code;

        if matches!(
            name,
            Some(
                StrId::INVARIANT
                    | StrId::INVARIANT_VIOLATION
                    | StrId::TRIGGER_ERROR
                    | StrId::FUNCTION_EXISTS
                    | StrId::CLASS_EXISTS
                    | StrId::SET_FRAME_METADATA
                    | StrId::LIB_C_FIRSTX
                    | StrId::LIB_C_LASTX
                    | StrId::LIB_C_ONLYX
            )
        ) {
            functionlike_storage.has_throw = true;
        }

        functionlike_storage.type_resolution_context = Some(type_resolution_context);

        if !self.user_defined {
            if let Some(name) = name {
                fix_function_return_type(name, &mut functionlike_storage);
            }
        }

        functionlike_storage
    }
}

fn fix_function_return_type(function_name: StrId, functionlike_storage: &mut FunctionLikeInfo) {
    match function_name {
        // bool
        StrId::HASH_EQUALS | StrId::IN_ARRAY => {
            functionlike_storage.return_type = Some(get_bool());
        }

        // int
        StrId::MB_STRLEN | StrId::RAND => functionlike_storage.return_type = Some(get_int()),

        // string
        StrId::UTF8_ENCODE
        | StrId::SHA1
        | StrId::DIRNAME
        | StrId::VSPRINTF
        | StrId::TRIM
        | StrId::LTRIM
        | StrId::RTRIM
        | StrId::STRPAD
        | StrId::STR_REPLACE
        | StrId::MD5
        | StrId::BASENAME
        | StrId::STRTOLOWER
        | StrId::STRTOUPPER
        | StrId::MB_STRTOLOWER
        | StrId::MB_STRTOUPPER
        | StrId::DATE => functionlike_storage.return_type = Some(get_string()),

        // falsable strings
        StrId::JSON_ENCODE
        | StrId::FILE_GET_CONTENTS
        | StrId::HEX2BIN
        | StrId::REALPATH
        | StrId::BASE64_DECODE
        | StrId::DATE_FORMAT
        | StrId::HASH_HMAC => {
            let mut false_or_string = TUnion::new(vec![TAtomic::TString, TAtomic::TFalse]);
            false_or_string.ignore_falsable_issues = true;
            functionlike_storage.return_type = Some(false_or_string);
        }

        // falsable ints
        StrId::STRTOTIME | StrId::MKTIME => {
            let mut false_or_int = TUnion::new(vec![TAtomic::TInt, TAtomic::TFalse]);
            false_or_int.ignore_falsable_issues = true;
            functionlike_storage.return_type = Some(false_or_int);
        }

        // falsable strings
        StrId::PASSWORD_HASH => {
            let mut false_or_null_or_string = TUnion::new(vec![
                TAtomic::TStringWithFlags(false, true, false),
                TAtomic::TFalse,
                TAtomic::TNull,
            ]);
            false_or_null_or_string.ignore_falsable_issues = true;
            functionlike_storage.return_type = Some(false_or_null_or_string);
        }
        _ => {}
    }
}

fn get_uses_hash(uses: &Vec<(StrId, StrId)>) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    uses.hash(&mut hasher);
    hasher.finish()
}

fn get_function_hashes(
    file_contents: &str,
    def_location: &HPos,
    name: &Id,
    tparams: &[Tparam],
    params: &[FunParam],
    user_attributes: &[AttributeInfo],
    suppressed_issues: Vec<&IssueKind>,
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
        if let tast::FunParamInfo::ParamOptional(Some(expr)) = &last_param.info {
            signature_end = expr.1.end_offset();
        }

        if let Some(last_hint) = &last_param.type_hint.1 {
            signature_end = last_hint.0.end_offset();
        }
    }

    if let Some(ret_hint) = &ret.1 {
        signature_end = ret_hint.0.end_offset();
    }

    let mut signature_hash = xxhash_rust::xxh3::xxh3_64(
        file_contents[def_location.start_offset as usize..signature_end].as_bytes(),
    );

    for issue in suppressed_issues {
        let mut hasher = rustc_hash::FxHasher::default();
        issue.hash(&mut hasher);
        signature_hash = signature_hash.wrapping_add(hasher.finish());
    }

    for attribute in user_attributes {
        signature_hash = signature_hash
            .wrapping_add(xxhash_rust::xxh3::xxh3_64(&attribute.name.0.to_le_bytes()));
    }

    (
        signature_hash,
        xxhash_rust::xxh3::xxh3_64(
            file_contents[signature_end..def_location.end_offset as usize].as_bytes(),
        )
        .wrapping_add(get_uses_hash(uses)),
    )
}

pub fn collect_info_for_aast(
    program: &aast::Program<(), ()>,
    resolved_names: &FxHashMap<u32, StrId>,
    interner: &mut ThreadedInterner,
    codebase: &mut CodebaseInfo,
    all_custom_issues: &FxHashSet<String>,
    file_source: FileSource,
    user_defined: bool,
    uses: Uses,
    parser_errors: Vec<ParserError>,
) {
    let file_path_id = file_source.file_path;

    let mut checker = Scanner {
        codebase,
        interner,
        file_source,
        resolved_names,
        user_defined,
        all_custom_issues,
        closure_refs: vec![],
        ast_nodes: Vec::new(),
        uses,
    };

    let mut context = Context {
        classlike_name: None,
        function_name: None,
        member_name: None,
        has_yield: false,
        has_throw: false,
        has_asio_join: false,
        has_static_field_access: false,
        uses_position: None,
        namespace_position: None,
    };
    visit(&mut checker, &mut context, program).unwrap();

    if user_defined {
        checker.ast_nodes.shrink_to_fit();
        checker.codebase.files.insert(
            file_path_id,
            FileInfo {
                closure_refs: checker.closure_refs,
                ast_nodes: checker.ast_nodes,
                parser_errors,
            },
        );
    }
}
