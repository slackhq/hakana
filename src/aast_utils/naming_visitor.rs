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
    pub interner: &'a mut ThreadedInterner,
}

impl<'ast> Visitor<'ast> for Scanner<'_> {
    type Params = AstParams<NameContext, ()>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params = Self::Params> {
        self
    }

    fn visit_def(&mut self, nc: &mut NameContext, p: &aast::Def<(), ()>) -> Result<(), ()> {
        match p {
            aast::Def::Namespace(ns) => {
                if !ns.0 .1.is_empty() {
                    nc.start_namespace(ns.0 .1.clone());
                }
            }
            aast::Def::NamespaceUse(uses) => {
                for (ns_kind, name, alias_name) in uses {
                    nc.add_alias(self.interner, name.1.clone(), alias_name.1.clone(), ns_kind);
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

    fn visit_class_(&mut self, nc: &mut NameContext, c: &aast::Class_<(), ()>) -> Result<(), ()> {
        let namespace_name = nc.get_namespace_name();

        let p = if let Some(namespace_name) = namespace_name {
            let str = namespace_name.clone() + "\\" + c.name.1.as_str();
            self.interner.intern(str)
        } else {
            if c.is_xhp {
                self.interner.intern(c.name.1.replace(":", "\\"))
            } else {
                self.interner.intern(c.name.1.clone())
            }
        };

        self.resolved_names.insert(c.name.0.start_offset(), p);

        nc.symbol_name = Some(p);
        let result = c.recurse(nc, self);
        nc.symbol_name = None;

        result
    }

    fn visit_typedef(&mut self, nc: &mut NameContext, t: &aast::Typedef<(), ()>) -> Result<(), ()> {
        let namespace_name = nc.get_namespace_name();

        let p = if let Some(namespace_name) = namespace_name {
            let str = namespace_name.clone() + "\\" + t.name.1.as_str();
            self.interner.intern(str)
        } else {
            self.interner.intern(t.name.1.clone())
        };

        self.resolved_names.insert(t.name.0.start_offset(), p);

        nc.symbol_name = Some(p);
        let result = t.recurse(nc, self);
        nc.symbol_name = None;

        result
    }

    fn visit_shape_field_info(
        &mut self,
        nc: &mut NameContext,
        p: &oxidized::tast::ShapeFieldInfo,
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
        nc: &mut NameContext,
        id: &aast::ClassId_<(), ()>,
    ) -> Result<(), ()> {
        let was_in_class_id = nc.in_class_id;

        nc.in_class_id = true;

        let result = id.recurse(nc, self);

        nc.in_class_id = was_in_class_id;

        result
    }

    fn visit_expr_(&mut self, nc: &mut NameContext, e: &aast::Expr_<(), ()>) -> Result<(), ()> {
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
            aast::Expr_::Call(boxed) => match &boxed.0 .2 {
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
        nc: &mut NameContext,
        p: &oxidized::nast::ShapeFieldName,
    ) -> Result<(), ()> {
        match p {
            oxidized::nast::ShapeFieldName::SFclassConst(id, _) => {
                let resolved_name =
                    nc.get_resolved_name(self.interner, &id.1, aast::NsKind::NSClass);

                self.resolved_names
                    .insert(id.0.start_offset(), resolved_name);
            }
            _ => {}
        };

        p.recurse(nc, self)
    }

    fn visit_catch(
        &mut self,
        nc: &mut NameContext,
        catch: &oxidized::nast::Catch,
    ) -> Result<(), ()> {
        nc.in_class_id = true;
        catch.recurse(nc, self)
    }

    fn visit_function_ptr_id(
        &mut self,
        nc: &mut NameContext,
        p: &aast::FunctionPtrId<(), ()>,
    ) -> Result<(), ()> {
        nc.in_function_id = true;
        p.recurse(nc, self)
    }

    fn visit_id(&mut self, nc: &mut NameContext, id: &ast_defs::Id) -> Result<(), ()> {
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
            if !self.resolved_names.contains_key(&id.0.start_offset()) {
                let resolved_name = if nc.in_xhp_id {
                    nc.get_resolved_name(
                        self.interner,
                        &id.1[1..].replace(":", "\\"),
                        aast::NsKind::NSClassAndNamespace,
                    )
                } else {
                    nc.get_resolved_name(
                        self.interner,
                        &id.1,
                        if nc.in_class_id || nc.in_constant_id {
                            aast::NsKind::NSClassAndNamespace
                        } else {
                            aast::NsKind::NSFun
                        },
                    )
                };

                self.resolved_names
                    .insert(id.0.start_offset(), resolved_name);
            }

            nc.in_class_id = false;
            nc.in_xhp_id = false;
            nc.in_function_id = false;
            nc.in_constant_id = false;
        }

        id.recurse(nc, self)
    }

    fn visit_fun_(&mut self, nc: &mut NameContext, f: &aast::Fun_<(), ()>) -> Result<(), ()> {
        let namespace_name = nc.get_namespace_name();

        let p = if let Some(namespace_name) = namespace_name {
            let str = namespace_name.clone() + "\\" + f.name.1.as_str();
            self.interner.intern(str)
        } else {
            self.interner.intern(f.name.1.clone())
        };

        self.resolved_names.insert(f.name.0.start_offset(), p);

        nc.symbol_name = Some(p);
        let result = f.recurse(nc, self);
        nc.symbol_name = None;

        result
    }

    fn visit_user_attribute(
        &mut self,
        nc: &mut NameContext,
        c: &aast::UserAttribute<(), ()>,
    ) -> Result<(), ()> {
        nc.in_class_id = true;
        c.recurse(nc, self)
    }

    fn visit_gconst(&mut self, nc: &mut NameContext, c: &aast::Gconst<(), ()>) -> Result<(), ()> {
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

    fn visit_hint_(&mut self, nc: &mut NameContext, p: &aast::Hint_) -> Result<(), ()> {
        let happly = p.as_happly();

        if let Some(happly) = happly {
            if !NameContext::is_reserved(&happly.0 .1) {
                let resolved_name = nc.get_resolved_name(
                    self.interner,
                    &happly.0 .1,
                    aast::NsKind::NSClassAndNamespace,
                );

                self.resolved_names
                    .insert(happly.0 .0.start_offset(), resolved_name);
            }
        }

        let was_in_namespaced_symbol_id = nc.in_function_id;

        nc.in_function_id = false;

        let result = p.recurse(nc, self);

        nc.in_function_id = was_in_namespaced_symbol_id;

        result
    }
}
