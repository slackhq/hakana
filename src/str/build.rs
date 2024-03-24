use std::env;
use std::fs::File;
use std::io::{Result, Write};
use std::path::Path;

fn main() -> Result<()> {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("interned_strings.rs");
    let mut f = File::create(dest_path)?;

    let strings = vec![
        "",
        "<anonymous function>",
        "<aria attribute>",
        "<data attribute>",
        "Codegen",
        "DOMDocument",
        "DateTime",
        "DateTimeImmutable",
        "HH\\AnyArray",
        "HH\\Asio\\join",
        "HH\\AsyncIterator",
        "HH\\AsyncKeyedIterator",
        "HH\\Awaitable",
        "HH\\BuiltinEnum",
        "HH\\BuiltinEnumClass",
        "HH\\Container",
        "HH\\EnumClass\\Label",
        "HH\\FormatString",
        "HH\\Iterator",
        "HH\\KeyedContainer",
        "HH\\KeyedIterator",
        "HH\\KeyedTraversable",
        "HH\\Lib\\C\\any",
        "HH\\Lib\\C\\contains",
        "HH\\Lib\\C\\contains_key",
        "HH\\Lib\\C\\count",
        "HH\\Lib\\C\\every",
        "HH\\Lib\\C\\find",
        "HH\\Lib\\C\\find_key",
        "HH\\Lib\\C\\findx",
        "HH\\Lib\\C\\first",
        "HH\\Lib\\C\\first_key",
        "HH\\Lib\\C\\first_keyx",
        "HH\\Lib\\C\\firstx",
        "HH\\Lib\\C\\is_empty",
        "HH\\Lib\\C\\last",
        "HH\\Lib\\C\\last_key",
        "HH\\Lib\\C\\last_keyx",
        "HH\\Lib\\C\\lastx",
        "HH\\Lib\\C\\onlyx",
        "HH\\Lib\\C\\search",
        "HH\\Lib\\Dict\\associate",
        "HH\\Lib\\Dict\\chunk",
        "HH\\Lib\\Dict\\contains",
        "HH\\Lib\\Dict\\contains_key",
        "HH\\Lib\\Dict\\diff_by_key",
        "HH\\Lib\\Dict\\fill_keys",
        "HH\\Lib\\Dict\\filter",
        "HH\\Lib\\Dict\\filter_async",
        "HH\\Lib\\Dict\\filter_keys",
        "HH\\Lib\\Dict\\filter_nulls",
        "HH\\Lib\\Dict\\filter_with_key",
        "HH\\Lib\\Dict\\flatten",
        "HH\\Lib\\Dict\\flip",
        "HH\\Lib\\Dict\\from_async",
        "HH\\Lib\\Dict\\from_entries",
        "HH\\Lib\\Dict\\from_keys",
        "HH\\Lib\\Dict\\from_keys_async",
        "HH\\Lib\\Dict\\map",
        "HH\\Lib\\Dict\\map_async",
        "HH\\Lib\\Dict\\map_with_key",
        "HH\\Lib\\Dict\\map_with_key_async",
        "HH\\Lib\\Dict\\merge",
        "HH\\Lib\\Dict\\reverse",
        "HH\\Lib\\Dict\\select_keys",
        "HH\\Lib\\Dict\\take",
        "HH\\Lib\\Keyset\\chunk",
        "HH\\Lib\\Keyset\\diff",
        "HH\\Lib\\Keyset\\equal",
        "HH\\Lib\\Keyset\\filter",
        "HH\\Lib\\Keyset\\filter_async",
        "HH\\Lib\\Keyset\\filter_nulls",
        "HH\\Lib\\Keyset\\flatten",
        "HH\\Lib\\Keyset\\intersect",
        "HH\\Lib\\Keyset\\keys",
        "HH\\Lib\\Keyset\\map",
        "HH\\Lib\\Keyset\\map_async",
        "HH\\Lib\\Keyset\\map_with_key",
        "HH\\Lib\\Keyset\\take",
        "HH\\Lib\\Keyset\\union",
        "HH\\Lib\\Math\\INT32_MAX",
        "HH\\Lib\\Math\\abs",
        "HH\\Lib\\Math\\almost_equals",
        "HH\\Lib\\Math\\base_convert",
        "HH\\Lib\\Math\\ceil",
        "HH\\Lib\\Math\\cos",
        "HH\\Lib\\Math\\exp",
        "HH\\Lib\\Math\\floor",
        "HH\\Lib\\Math\\from_base",
        "HH\\Lib\\Math\\int_div",
        "HH\\Lib\\Math\\is_nan",
        "HH\\Lib\\Math\\log",
        "HH\\Lib\\Math\\max",
        "HH\\Lib\\Math\\max_by",
        "HH\\Lib\\Math\\maxva",
        "HH\\Lib\\Math\\mean",
        "HH\\Lib\\Math\\median",
        "HH\\Lib\\Math\\min",
        "HH\\Lib\\Math\\min_by",
        "HH\\Lib\\Math\\minva",
        "HH\\Lib\\Math\\round",
        "HH\\Lib\\Math\\sin",
        "HH\\Lib\\Math\\sqrt",
        "HH\\Lib\\Math\\sum",
        "HH\\Lib\\Math\\sum_float",
        "HH\\Lib\\Math\\tan",
        "HH\\Lib\\Math\\to_base",
        "HH\\Lib\\Regex\\Pattern",
        "HH\\Lib\\Regex\\every_match",
        "HH\\Lib\\Regex\\first_match",
        "HH\\Lib\\Regex\\matches",
        "HH\\Lib\\Regex\\replace",
        "HH\\Lib\\Str\\capitalize",
        "HH\\Lib\\Str\\capitalize_words",
        "HH\\Lib\\Str\\chunk",
        "HH\\Lib\\Str\\compare",
        "HH\\Lib\\Str\\compare_ci",
        "HH\\Lib\\Str\\contains",
        "HH\\Lib\\Str\\contains_ci",
        "HH\\Lib\\Str\\ends_with",
        "HH\\Lib\\Str\\ends_with_ci",
        "HH\\Lib\\Str\\format",
        "HH\\Lib\\Str\\format_number",
        "HH\\Lib\\Str\\is_empty",
        "HH\\Lib\\Str\\join",
        "HH\\Lib\\Str\\length",
        "HH\\Lib\\Str\\lowercase",
        "HH\\Lib\\Str\\pad_left",
        "HH\\Lib\\Str\\pad_right",
        "HH\\Lib\\Str\\repeat",
        "HH\\Lib\\Str\\replace",
        "HH\\Lib\\Str\\replace_ci",
        "HH\\Lib\\Str\\replace_every",
        "HH\\Lib\\Str\\search",
        "HH\\Lib\\Str\\slice",
        "HH\\Lib\\Str\\split",
        "HH\\Lib\\Str\\starts_with",
        "HH\\Lib\\Str\\starts_with_ci",
        "HH\\Lib\\Str\\strip_prefix",
        "HH\\Lib\\Str\\strip_suffix",
        "HH\\Lib\\Str\\to_int",
        "HH\\Lib\\Str\\trim",
        "HH\\Lib\\Str\\trim_left",
        "HH\\Lib\\Str\\trim_right",
        "HH\\Lib\\Str\\uppercase",
        "HH\\Lib\\Vec\\chunk",
        "HH\\Lib\\Vec\\concat",
        "HH\\Lib\\Vec\\diff",
        "HH\\Lib\\Vec\\drop",
        "HH\\Lib\\Vec\\filter",
        "HH\\Lib\\Vec\\filter_async",
        "HH\\Lib\\Vec\\filter_nulls",
        "HH\\Lib\\Vec\\filter_with_key",
        "HH\\Lib\\Vec\\flatten",
        "HH\\Lib\\Vec\\from_async",
        "HH\\Lib\\Vec\\intersect",
        "HH\\Lib\\Vec\\keys",
        "HH\\Lib\\Vec\\map",
        "HH\\Lib\\Vec\\map_async",
        "HH\\Lib\\Vec\\map_with_key",
        "HH\\Lib\\Vec\\range",
        "HH\\Lib\\Vec\\reverse",
        "HH\\Lib\\Vec\\slice",
        "HH\\Lib\\Vec\\sort",
        "HH\\Lib\\Vec\\take",
        "HH\\Lib\\Vec\\unique",
        "HH\\Lib\\Vec\\zip",
        "HH\\MemberOf",
        "HH\\Shapes",
        "HH\\Traversable",
        "HH\\TypeStructure",
        "HH\\Vector",
        "HH\\dict",
        "HH\\global_get",
        "HH\\idx",
        "HH\\invariant",
        "HH\\invariant_violation",
        "HH\\keyset",
        "HH\\set_frame_metadata",
        "HH\\type_structure",
        "HH\\vec",
        "Hakana\\FindPaths\\Sanitize",
        "Hakana\\Immutable",
        "Hakana\\MustUse",
        "Hakana\\SecurityAnalysis\\IgnorePath",
        "Hakana\\SecurityAnalysis\\IgnorePathIfTrue",
        "Hakana\\SecurityAnalysis\\RemoveTaintsWhenReturningTrue",
        "Hakana\\SecurityAnalysis\\Sanitize",
        "Hakana\\SecurityAnalysis\\ShapeSource",
        "Hakana\\SecurityAnalysis\\Sink",
        "Hakana\\SecurityAnalysis\\Source",
        "Hakana\\SecurityAnalysis\\SpecializeCall",
        "Hakana\\SpecialTypes\\LiteralString",
        "NumberFormatter",
        "SimpleXMLElement",
        "XHPChild",
        "__DIR__",
        "__DynamicallyCallable",
        "__EntryPoint",
        "__FILE__",
        "__FUNCTION__",
        "__PHP_Incomplete_Class",
        "__Sealed",
        "__construct",
        "addcslashes",
        "addslashes",
        "assert",
        "assertAll",
        "at",
        "base64_decode",
        "base64_encode",
        "basename",
        "bin2hex",
        "chop",
        "chunk_split",
        "class_exists",
        "coerce",
        "convert_uudecode",
        "convert_uuencode",
        "count",
        "crc32",
        "ctype_lower",
        "date",
        "date_format",
        "debug_backtrace",
        "dirname",
        "echo",
        "escapeshellarg",
        "explode",
        "extension",
        "file_get_contents",
        "filename",
        "filter_var",
        "fromItems",
        "function_exists",
        "get_class",
        "get_object_vars",
        "gzinflate",
        "hash",
        "hash_equals",
        "hash_hmac",
        "hex2bin",
        "highlight_string",
        "htmlentities",
        "htmlentitydecode",
        "htmlspecialchars",
        "htmlspecialchars_decode",
        "http_build_query",
        "idx",
        "implode",
        "in_array",
        "include",
        "intval",
        "ip2long",
        "isset",
        "join",
        "json_decode",
        "json_encode",
        "keyExists",
        "lcfirst",
        "log",
        "ltrim",
        "mb_strlen",
        "mb_strtolower",
        "mb_strtoupper",
        "md5",
        "microtime",
        "mktime",
        "nl2br",
        "number_format",
        "ord",
        "parent",
        "password_hash",
        "pathinfo",
        "preg_filter",
        "preg_grep",
        "preg_match",
        "preg_match_all_with_matches",
        "preg_match_with_matches",
        "preg_quote",
        "preg_replace",
        "preg_replace_with_count",
        "preg_split",
        "print_r",
        "printf",
        "quote_meta",
        "quoted_printable_decode",
        "quoted_printable_encode",
        "rand",
        "range",
        "rawurlencode",
        "realpath",
        "removeKey",
        "rtrim",
        "self",
        "serialize",
        "sha1",
        "sscanf",
        "static",
        "stdClass",
        "str_ireplace",
        "str_pad",
        "str_repeat",
        "str_replace",
        "str_rot13",
        "str_shuffle",
        "str_split",
        "strchr",
        "strcmp",
        "strgetcsv",
        "strip_tags",
        "stripcslashes",
        "stripslashes",
        "stristr",
        "strnatcasecmp",
        "strpad",
        "strpbrk",
        "strpos",
        "strrchr",
        "strrev",
        "strstr",
        "strtolower",
        "strtotime",
        "strtoupper",
        "strval",
        "substr",
        "substr_count",
        "substr_replace",
        "this",
        "toArray",
        "toDict",
        "trigger_error",
        "trim",
        "ucfirst",
        "ucwords",
        "unset",
        "urldecode",
        "urlencode",
        "utf8_encode",
        "var_dump",
        "var_export",
        "vsprintf",
        "wordwrap",
    ];

    writeln!(f, "impl StrId {{")?;
    for (i, name) in strings.iter().enumerate() {
        let const_name = format_identifier(name);
        writeln!(f, "    pub const {}: StrId = StrId({});", const_name, i)?;
    }
    writeln!(f, "}}")?;

    writeln!(f, "impl Default for Interner {{")?;
    writeln!(f, "    fn default() -> Self {{")?;
    writeln!(
        f,
        "        let mut interner = Interner {{ map: IndexSet::default() }};"
    )?;
    for name in &strings {
        writeln!(
            f,
            "        interner.intern(\"{}\".to_string());",
            name.replace('\\', "\\\\")
        )?;
    }
    writeln!(f, "        interner")?;
    writeln!(f, "    }}")?;
    writeln!(f, "}}")?;

    Ok(())
}

fn format_identifier(input: &str) -> String {
    if input.is_empty() {
        return "EMPTY".to_string();
    }

    if input == "HH\\type_structure" {
        return "TYPE_STRUCTURE_FN".to_string();
    }

    if input == "HH\\idx" {
        return "IDX_FN".to_string();
    }

    if input.starts_with("__") && input.ends_with("__") {
        return input[2..input.len() - 2].to_string() + "_CONST";
    }

    let mut formatted_input = input.to_string();

    // Strip "HH\\" prefix if present
    formatted_input = formatted_input
        .trim_start_matches("HH\\")
        .trim_start_matches("__")
        .to_string();

    // Replace "\\" with "_" for namespaced constants
    formatted_input = formatted_input
        .replace('\\', "_")
        .replace(['<', '>'], "")
        .replace(' ', "_");

    let mut result = String::new();
    let mut was_lower = false;

    for (i, ch) in formatted_input.chars().enumerate() {
        // Convert camelCase to CAMEL_CASE
        if ch.is_uppercase() {
            if i != 0 && was_lower {
                result.push('_');
            }
            result.extend(ch.to_lowercase());
        } else {
            result.push(ch);
        }

        was_lower = ch.is_lowercase();
    }

    // Convert to uppercase
    result
        .to_uppercase()
        .replace("XHPCHILD", "XHP_CHILD")
        .replace("SIMPLE_XMLELEMENT", "SIMPLE_XML_ELEMENT")
}
