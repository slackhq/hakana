use aast_parser::rust_aast_parser_types::Env as AastParserEnv;
use name_context::NameContext;
use ocamlrep::rc::RcOc;
use oxidized::ast_defs::Pos;
use oxidized::prim_defs::Comment;
use oxidized::relative_path::{Prefix, RelativePath};
use oxidized::scoured_comments::ScouredComments;
use oxidized::{
    aast,
    aast_visitor::{visit, AstParams, Node, Visitor},
    ast_defs,
};
use parser_core_types::{indexed_source_text::IndexedSourceText, source_text::SourceText};
use rustc_hash::FxHashMap;
use std::fs::File;
use std::io::Write;
use std::{
    fs,
    path::{Path, PathBuf},
};

mod name_context;

pub fn get_aast_for_path_and_contents(
    local_path: String,
    file_contents: String,
    aast_cache_dir: Option<String>,
    has_changed: bool,
) -> Result<(aast::Program<(), ()>, ScouredComments), String> {
    let path_hash = xxhash_rust::xxh3::xxh3_64(local_path.as_bytes());

    let cache_path = if let Some(cache_dir) = aast_cache_dir {
        if !Path::new(&cache_dir).is_dir() {
            if fs::create_dir(&cache_dir).is_err() {
                panic!("could not create aast cache directory");
            }
        }

        Some(format!("{}/{:x}", cache_dir, path_hash))
    } else {
        None
    };

    if !has_changed {
        if let Some(cache_path) = &cache_path {
            if Path::new(&cache_path).exists() {
                let serialized_aast = fs::read(&cache_path)
                    .unwrap_or_else(|_| panic!("Could not read file {}", &cache_path));
                if let Ok(aast) = bincode::deserialize::<(aast::Program<(), ()>, ScouredComments)>(
                    &serialized_aast,
                ) {
                    return Ok(aast);
                }
            }
        }
    }

    let rc_path = RcOc::new(RelativePath::make(Prefix::Root, PathBuf::from(&local_path)));

    let text = SourceText::make(rc_path.clone(), file_contents.as_bytes());
    let indexed_source_text = IndexedSourceText::new(text.clone());

    let mut parser_env = AastParserEnv::default();
    parser_env.keep_errors = true;
    //parser_env.include_line_comments = true;

    let mut parser_result = if let Ok(parser_result) =
        aast_parser::AastParser::from_text(&parser_env, &indexed_source_text, None)
    {
        parser_result
    } else {
        return Err("Not a valid Hack file".to_string());
    };

    let aast = parser_result.aast;

    // rewrite positional data for comments because it comes out wrong in the AST
    for (pos, comment) in parser_result.scoured_comments.comments.iter_mut() {
        match comment {
            Comment::CmtLine(_) => {
                let mut offsets = pos.to_start_and_end_lnum_bol_offset();
                offsets.0 .2 -= 2;
                *pos = Pos::from_lnum_bol_offset(rc_path.clone(), offsets.0, offsets.1);
            }
            Comment::CmtBlock(text) => {
                let mut offsets = pos.to_start_and_end_lnum_bol_offset();
                let newline_count = text.as_bytes().iter().filter(|&&c| c == b'\n').count();
                let comment_length = text.len();

                offsets.0 .0 -= newline_count;
                offsets.0 .2 -= comment_length + 2;
                if newline_count > 0 {
                    // we lose the true bol here for the comment, which is a shame
                    offsets.0 .1 = offsets.0 .2;
                }
                offsets.1 .2 += 1;
                *pos = Pos::from_lnum_bol_offset(rc_path.clone(), offsets.0, offsets.1);
            }
        }
    }

    match aast {
        Ok(aast) => {
            if let Some(cache_path) = cache_path {
                let mut file = File::create(&cache_path).unwrap();
                let serialized_aast =
                    bincode::serialize(&(&aast, &parser_result.scoured_comments)).unwrap();
                file.write_all(&serialized_aast)
                    .unwrap_or_else(|_| panic!("Could not write file {}", &cache_path));
            }

            Ok((aast, parser_result.scoured_comments))
        }
        Err(string) => Err(string),
    }
}

struct Scanner {
    pub resolved_names: FxHashMap<usize, String>,
}

impl Scanner {
    fn new() -> Self {
        Self {
            resolved_names: FxHashMap::default(),
        }
    }
}

impl<'ast> Visitor<'ast> for Scanner {
    type Params = AstParams<NameContext, ()>;

    fn object(&mut self) -> &mut dyn Visitor<'ast, Params = Self::Params> {
        self
    }

    fn visit_def(&mut self, nc: &mut NameContext, p: &aast::Def<(), ()>) -> Result<(), ()> {
        if p.is_namespace() {
            let ns = p.as_namespace().unwrap();
            if !ns.0 .1.is_empty() {
                nc.start_namespace(ns.0 .1.clone());
            }
        }

        if p.is_namespace_use() {
            for (ns_kind, name, alias_name) in p.as_namespace_use().unwrap() {
                nc.add_alias(name.1.clone(), alias_name.1.clone(), ns_kind);
            }
        }

        let result = p.recurse(nc, self);

        if p.is_namespace() {
            nc.end_namespace();
        }

        result
    }

    fn visit_class_(&mut self, nc: &mut NameContext, c: &aast::Class_<(), ()>) -> Result<(), ()> {
        let namespace_name = nc.get_namespace_name();

        self.resolved_names.insert(
            c.name.0.start_offset(),
            if let Some(namespace_name) = namespace_name {
                format!("{}\\{}", namespace_name, c.name.1)
            } else {
                c.name.1.clone()
            },
        );

        c.recurse(nc, self)
    }

    fn visit_typedef(&mut self, nc: &mut NameContext, t: &aast::Typedef<(), ()>) -> Result<(), ()> {
        let namespace_name = nc.get_namespace_name();

        self.resolved_names.insert(
            t.name.0.start_offset(),
            if let Some(namespace_name) = namespace_name {
                format!("{}\\{}", namespace_name, t.name.1)
            } else {
                t.name.1.clone()
            },
        );

        t.recurse(nc, self)
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
        if let aast::Expr_::Xml(_) = e {
            nc.in_xhp_id = true;
        }

        let result = e.recurse(nc, self);

        if let aast::Expr_::Xml(_) = e {
            nc.in_xhp_id = false;
        }

        result
    }

    fn visit_id(&mut self, nc: &mut NameContext, id: &ast_defs::Id) -> Result<(), ()> {
        if !self.resolved_names.contains_key(&id.0.start_offset()) {
            let resolved_name = if nc.in_xhp_id {
                nc.get_resolved_name(&id.1[1..].to_string(), aast::NsKind::NSClassAndNamespace)
            } else {
                nc.get_resolved_name(
                    &id.1,
                    if nc.in_class_id || nc.in_nonfunction_id {
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

        id.recurse(nc, self)
    }

    fn visit_fun_(&mut self, nc: &mut NameContext, f: &aast::Fun_<(), ()>) -> Result<(), ()> {
        let namespace_name = nc.get_namespace_name();

        self.resolved_names.insert(
            f.name.0.start_offset(),
            if let Some(namespace_name) = namespace_name {
                format!("{}\\{}", namespace_name, f.name.1)
            } else {
                f.name.1.clone()
            },
        );

        f.recurse(nc, self)
    }

    fn visit_hint_(&mut self, nc: &mut NameContext, p: &aast::Hint_) -> Result<(), ()> {
        let happly = p.as_happly();

        if let Some(happly) = happly {
            if !NameContext::is_reserved(&happly.0 .1) {
                let resolved_name =
                    nc.get_resolved_name(&happly.0 .1, aast::NsKind::NSClassAndNamespace);
                self.resolved_names
                    .insert(happly.0 .0.start_offset(), resolved_name);
            }
        }

        let was_in_nonfunction_id = nc.in_nonfunction_id;

        nc.in_nonfunction_id = true;

        let result = p.recurse(nc, self);

        nc.in_nonfunction_id = was_in_nonfunction_id;

        result
    }
}

pub fn scope_names(program: &aast::Program<(), ()>) -> FxHashMap<usize, String> {
    let mut scanner = Scanner::new();
    let mut context = NameContext::new();
    visit(&mut scanner, &mut context, program).unwrap();
    scanner.resolved_names
}
