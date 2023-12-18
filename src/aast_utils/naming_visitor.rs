use hakana_reflection_info::{StrId, ThreadedInterner};

use oxidized::{
    aast,
    aast_visitor::{AstParams, Node, Visitor},
    ast_defs,
};
use rustc_hash::FxHashMap;

use crate::name_context::NameContext;

pub(crate) struct Scanner<'a> {
    pub resolved_names: FxHashMap<usize, StrId>,
    pub symbol_uses: FxHashMap<StrId, Vec<(StrId, StrId)>>,
    pub symbol_member_uses: FxHashMap<(StrId, StrId), Vec<(StrId, StrId)>>,
    pub file_uses: Vec<(StrId, StrId)>,
    pub interner: &'a mut ThreadedInterner,
}

impl<'ast> Visitor<'ast> for Scanner<'_> {
    type Params = AstParams<NameContext<'ast>, ()>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params = Self::Params> {
        self
    }

    fn visit_def(
        &mut self,
        nc: &mut NameContext<'ast>,
        p: &'ast aast::Def<(), ()>,
    ) -> Result<(), ()> {
        match p {
            aast::Def::Namespace(ns) => {
                if !ns.0 .1.is_empty() {
                    nc.start_namespace(ns.0 .1.clone());
                }
            }
            aast::Def::NamespaceUse(uses) => {
                for (ns_kind, name, alias_name) in uses {
                    nc.add_alias(
                        self.interner,
                        if name.1.starts_with('\\') {
                            &name.1[1..]
                        } else {
                            &name.1
                        },
                        &alias_name.1,
                        ns_kind,
                    );
                }
            }
            _ => {}
        }

        let result = p.recurse(nc, self);

        if let aast::Def::Namespace(_) = p {
            nc.end_namespace()
        }

        result
    }

    fn visit_class_(
        &mut self,
        nc: &mut NameContext<'ast>,
        c: &'ast aast::Class_<(), ()>,
    ) -> Result<(), ()> {
        let namespace_name = nc.get_namespace_name();

        let p = if let Some(namespace_name) = namespace_name {
            let str = namespace_name.clone() + "\\" + c.name.1.as_str();
            self.interner.intern(str)
        } else if c.is_xhp {
            self.interner.intern(c.name.1.replace(':', "\\"))
        } else {
            self.interner.intern(c.name.1.clone())
        };

        self.resolved_names.insert(c.name.0.start_offset(), p);

        for type_param_node in &c.tparams {
            nc.generic_params.push(&type_param_node.name.1);
            self.resolved_names.insert(
                type_param_node.name.0.start_offset(),
                self.interner.intern_str(&type_param_node.name.1),
            );
        }

        nc.symbol_name = Some(p);
        let result = c.recurse(nc, self);
        nc.symbol_name = None;
        nc.generic_params = vec![];

        result
    }

    fn visit_typedef(
        &mut self,
        nc: &mut NameContext<'ast>,
        t: &'ast aast::Typedef<(), ()>,
    ) -> Result<(), ()> {
        let namespace_name = nc.get_namespace_name();

        let p = if let Some(namespace_name) = namespace_name {
            let str = namespace_name.clone() + "\\" + t.name.1.as_str();
            self.interner.intern(str)
        } else {
            self.interner.intern(t.name.1.clone())
        };

        self.resolved_names.insert(t.name.0.start_offset(), p);

        for type_param_node in &t.tparams {
            nc.generic_params.push(&type_param_node.name.1);
            self.resolved_names.insert(
                type_param_node.name.0.start_offset(),
                self.interner.intern_str(&type_param_node.name.1),
            );
        }

        nc.symbol_name = Some(p);
        let result = t.recurse(nc, self);
        nc.symbol_name = None;

        result
    }

    fn visit_shape_field_info(
        &mut self,
        nc: &mut NameContext<'ast>,
        p: &'ast oxidized::tast::ShapeFieldInfo,
    ) -> Result<(), ()> {
        match &p.name {
            oxidized::nast::ShapeFieldName::SFclassConst(_, member_name) => {
                let p = self.interner.intern(member_name.1.clone());
                self.resolved_names.insert(member_name.0.start_offset(), p);
            }
            _ => {}
        }
        p.recurse(nc, self)
    }

    fn visit_class_id_(
        &mut self,
        nc: &mut NameContext<'ast>,
        id: &'ast aast::ClassId_<(), ()>,
    ) -> Result<(), ()> {
        let was_in_class_id = nc.in_class_id;

        nc.in_class_id = true;

        let result = id.recurse(nc, self);

        nc.in_class_id = was_in_class_id;

        result
    }

    fn visit_xhp_simple(
        &mut self,
        nc: &mut NameContext<'ast>,
        p: &'ast oxidized::tast::XhpSimple<(), ()>,
    ) -> Result<(), ()> {
        if !p.name.1.starts_with("data-") && !p.name.1.starts_with("aria-") {
            let name = self.interner.intern(":".to_string() + &p.name.1);
            self.resolved_names.insert(p.name.0.start_offset(), name);
        }

        p.recurse(nc, self)
    }

    fn visit_expr_(
        &mut self,
        nc: &mut NameContext<'ast>,
        e: &'ast aast::Expr_<(), ()>,
    ) -> Result<(), ()> {
        let in_xhp_id = nc.in_xhp_id;

        let result = match e {
            aast::Expr_::Xml(_) => {
                nc.in_xhp_id = true;
                e.recurse(nc, self)
            }
            aast::Expr_::Id(_) => {
                nc.in_constant_id = true;
                e.recurse(nc, self)
            }
            aast::Expr_::EnumClassLabel(boxed) => {
                if boxed.0.is_some() {
                    nc.in_class_id = true;
                }
                e.recurse(nc, self)
            }
            aast::Expr_::Call(boxed) => match &boxed.func.2 {
                aast::Expr_::Id(_) => {
                    nc.in_function_id = true;
                    e.recurse(nc, self)
                }
                _ => e.recurse(nc, self),
            },
            aast::Expr_::ObjGet(boxed) => {
                boxed.0.recurse(nc, self).ok();
                nc.in_member_id = true;
                let result = boxed.1.recurse(nc, self);
                nc.in_member_id = false;
                result
            }
            _ => e.recurse(nc, self),
        };

        nc.in_class_id = false;
        nc.in_member_id = false;
        nc.in_function_id = false;
        nc.in_xhp_id = in_xhp_id;
        nc.in_constant_id = false;

        result
    }

    fn visit_shape_field_name(
        &mut self,
        nc: &mut NameContext<'ast>,
        p: &'ast oxidized::nast::ShapeFieldName,
    ) -> Result<(), ()> {
        match p {
            oxidized::nast::ShapeFieldName::SFclassConst(id, _) => {
                let resolved_name = nc.get_resolved_name(
                    self.interner,
                    &id.1,
                    aast::NsKind::NSClass,
                    if let Some(symbol_name) = nc.symbol_name {
                        if let Some(member_name) = nc.member_name {
                            self.symbol_member_uses
                                .entry((symbol_name, member_name))
                                .or_default()
                        } else {
                            self.symbol_uses.entry(symbol_name).or_default()
                        }
                    } else {
                        &mut self.file_uses
                    },
                );

                self.resolved_names
                    .insert(id.0.start_offset(), resolved_name);
            }
            _ => {}
        };

        p.recurse(nc, self)
    }

    fn visit_catch(
        &mut self,
        nc: &mut NameContext<'ast>,
        catch: &'ast oxidized::nast::Catch,
    ) -> Result<(), ()> {
        nc.in_class_id = true;
        catch.recurse(nc, self)
    }

    fn visit_function_ptr_id(
        &mut self,
        nc: &mut NameContext<'ast>,
        p: &'ast aast::FunctionPtrId<(), ()>,
    ) -> Result<(), ()> {
        nc.in_function_id = true;
        p.recurse(nc, self)
    }

    fn visit_id(&mut self, nc: &mut NameContext<'ast>, id: &'ast ast_defs::Id) -> Result<(), ()> {
        if nc.in_function_id {
            nc.in_constant_id = false;
        }

        if nc.in_member_id {
            nc.in_constant_id = false;
        }

        // println!(
        //     "{:#?} in_class_id:{} in_function_id:{} in_xhp_id:{} in_constant_id:{} in_member_id:{}",
        //     id, nc.in_class_id, nc.in_function_id, nc.in_xhp_id, nc.in_constant_id, nc.in_member_id
        // );

        if nc.in_class_id || nc.in_function_id || nc.in_xhp_id || nc.in_constant_id {
            if let std::collections::hash_map::Entry::Vacant(e) =
                self.resolved_names.entry(id.0.start_offset())
            {
                let resolved_name = if nc.in_xhp_id {
                    nc.get_resolved_name(
                        self.interner,
                        &id.1[1..].replace(':', "\\"),
                        aast::NsKind::NSClassAndNamespace,
                        if let Some(symbol_name) = nc.symbol_name {
                            if let Some(member_name) = nc.member_name {
                                self.symbol_member_uses
                                    .entry((symbol_name, member_name))
                                    .or_default()
                            } else {
                                self.symbol_uses.entry(symbol_name).or_default()
                            }
                        } else {
                            &mut self.file_uses
                        },
                    )
                } else {
                    nc.get_resolved_name(
                        self.interner,
                        &id.1,
                        if nc.in_constant_id && !nc.in_class_id {
                            aast::NsKind::NSConst
                        } else if nc.in_class_id {
                            aast::NsKind::NSClassAndNamespace
                        } else {
                            aast::NsKind::NSFun
                        },
                        if let Some(symbol_name) = nc.symbol_name {
                            if let Some(member_name) = nc.member_name {
                                self.symbol_member_uses
                                    .entry((symbol_name, member_name))
                                    .or_default()
                            } else {
                                self.symbol_uses.entry(symbol_name).or_default()
                            }
                        } else {
                            &mut self.file_uses
                        },
                    )
                };

                e.insert(resolved_name);
            }

            nc.in_class_id = false;
            nc.in_xhp_id = false;
            nc.in_function_id = false;
            nc.in_constant_id = false;
        }

        id.recurse(nc, self)
    }

    fn visit_fun_def(
        &mut self,
        nc: &mut NameContext<'ast>,
        f: &'ast aast::FunDef<(), ()>,
    ) -> Result<(), ()> {
        let namespace_name = nc.get_namespace_name();

        let p = if let Some(namespace_name) = namespace_name {
            let str = namespace_name.clone() + "\\" + f.name.1.as_str();
            self.interner.intern(str)
        } else {
            self.interner.intern(f.name.1.clone())
        };

        self.resolved_names.insert(f.name.0.start_offset(), p);

        for type_param_node in &f.tparams {
            nc.generic_params.push(&type_param_node.name.1);
            self.resolved_names.insert(
                type_param_node.name.0.start_offset(),
                self.interner.intern_str(&type_param_node.name.1),
            );
        }

        nc.symbol_name = Some(p);

        let result = f.recurse(nc, self);

        nc.symbol_name = None;
        nc.generic_params = vec![];

        result
    }

    fn visit_method_(
        &mut self,
        nc: &mut NameContext<'ast>,
        m: &'ast aast::Method_<(), ()>,
    ) -> Result<(), ()> {
        let p = self.interner.intern(m.name.1.clone());

        self.resolved_names.insert(m.name.0.start_offset(), p);

        let original_param_count = nc.generic_params.len();

        for type_param_node in &m.tparams {
            nc.generic_params.push(&type_param_node.name.1);
            self.resolved_names.insert(
                type_param_node.name.0.start_offset(),
                self.interner.intern_str(&type_param_node.name.1),
            );
        }

        nc.member_name = Some(p);
        let result = m.recurse(nc, self);
        nc.member_name = None;

        if !m.tparams.is_empty() {
            nc.generic_params.truncate(original_param_count);
        }

        result
    }

    fn visit_class_const(
        &mut self,
        nc: &mut NameContext<'ast>,
        c: &'ast aast::ClassConst<(), ()>,
    ) -> Result<(), ()> {
        let p = self.interner.intern(c.id.1.clone());

        self.resolved_names.insert(c.id.0.start_offset(), p);

        nc.member_name = Some(p);
        let result = c.recurse(nc, self);
        nc.member_name = None;

        result
    }

    fn visit_user_attribute(
        &mut self,
        nc: &mut NameContext<'ast>,
        c: &'ast aast::UserAttribute<(), ()>,
    ) -> Result<(), ()> {
        nc.in_class_id = true;
        c.recurse(nc, self)
    }

    fn visit_gconst(
        &mut self,
        nc: &mut NameContext<'ast>,
        c: &'ast aast::Gconst<(), ()>,
    ) -> Result<(), ()> {
        let namespace_name = nc.get_namespace_name();

        let p = if let Some(namespace_name) = namespace_name {
            let str = namespace_name.clone() + "\\" + c.name.1.as_str();
            self.interner.intern(str)
        } else {
            self.interner.intern(c.name.1.clone())
        };

        self.resolved_names.insert(c.name.0.start_offset(), p);

        nc.symbol_name = Some(p);
        let result = c.recurse(nc, self);
        nc.symbol_name = None;

        result
    }

    fn visit_hint_(&mut self, nc: &mut NameContext<'ast>, p: &'ast aast::Hint_) -> Result<(), ()> {
        match p {
            oxidized::tast::Hint_::Happly(id, _) => {
                if !NameContext::is_reserved(&id.1) {
                    let resolved_name = nc.get_resolved_name(
                        self.interner,
                        &id.1,
                        aast::NsKind::NSClassAndNamespace,
                        if let Some(symbol_name) = nc.symbol_name {
                            if let Some(member_name) = nc.member_name {
                                self.symbol_member_uses
                                    .entry((symbol_name, member_name))
                                    .or_default()
                            } else {
                                self.symbol_uses.entry(symbol_name).or_default()
                            }
                        } else {
                            &mut self.file_uses
                        },
                    );

                    self.resolved_names
                        .insert(id.0.start_offset(), resolved_name);
                }
            }
            oxidized::tast::Hint_::Haccess(_, const_names) => {
                for const_name in const_names {
                    let resolved_name = self.interner.intern(const_name.1.clone());

                    self.resolved_names
                        .insert(const_name.0.start_offset(), resolved_name);
                }
            }
            _ => {}
        }

        let was_in_namespaced_symbol_id = nc.in_function_id;

        nc.in_function_id = false;

        let result = p.recurse(nc, self);

        nc.in_function_id = was_in_namespaced_symbol_id;

        result
    }
}
