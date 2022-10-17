use ocamlrep::rc::RcOc;
use ocamlrep::Bump;
use oxidized::relative_path::{Prefix, RelativePath};
use parser_core_types::parser_env::ParserEnv;
use parser_core_types::{indexed_source_text::IndexedSourceText, source_text::SourceText};
use std::{
    path::{PathBuf},
};

pub fn get_aast_for_path_and_contents(
    local_path: String,
    file_contents: String,
) {
    let rc_path = RcOc::new(RelativePath::make(Prefix::Root, PathBuf::from(&local_path)));

    let text = SourceText::make(rc_path.clone(), file_contents.as_bytes());

    let bump = Bump::new();

    let mut tree = Some(positioned_by_ref_parser::parse_script(
        &bump,
        &text,
        ParserEnv {
            codegen: false,
            hhvm_compat_mode: true,
            php5_compat_mode: false,
            allow_new_attribute_syntax: true,
            enable_xhp_class_modifier: true,
            disable_xhp_element_mangling: true,
            disable_xhp_children_declarations: true,
            disallow_fun_and_cls_meth_pseudo_funcs: false,
            interpret_soft_types_as_like_types: false,
        },
        None,
    ).0);

    while let Some(tree) = &tree {
        
        if tree.is_class() {
            tree.get_token().unwrap().start_offset()
        }
    } 
}
