/// Options for parsing, with the names HHVM gives them. The first and the
/// last decide what is a syntax error (see `hakana_parser::ParserOptions`);
/// `disable_xhp_element_mangling` changes nothing about the syntax tree.
#[derive(Clone, Debug, Default)]
pub struct ParserEnv {
    pub enable_xhp_class_modifier: bool,
    pub disable_xhp_element_mangling: bool,
    pub disable_xhp_children_declarations: bool,
}
