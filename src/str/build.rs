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
        "HH\\Lib\\C\\contains",
        "HH\\Lib\\C\\contains_key",
        "HH\\Lib\\C\\firstx",
        "HH\\Lib\\C\\lastx",
        "HH\\Lib\\C\\onlyx",
        "HH\\Lib\\Dict\\contains",
        "HH\\Lib\\Dict\\contains_key",
        "HH\\Lib\\Math\\INT32_MAX",
        "HH\\Lib\\Regex\\Pattern",
        "HH\\Lib\\Regex\\matches",
        "HH\\Lib\\Str\\format",
        "HH\\Lib\\Str\\join",
        "HH\\Lib\\Str\\replace",
        "HH\\Lib\\Str\\slice",
        "HH\\Lib\\Str\\split",
        "HH\\Lib\\Str\\starts_with",
        "HH\\Lib\\Str\\strip_suffix",
        "HH\\Lib\\Str\\trim",
        "HH\\MemberOf",
        "HH\\Shapes",
        "HH\\Traversable",
        "HH\\TypeStructure",
        "HH\\Vector",
        "HH\\global_get",
        "HH\\idx",
        "HH\\invariant",
        "HH\\invariant_violation",
        "HH\\set_frame_metadata",
        "HH\\type_structure",
        "Hakana\\Immutable",
        "Hakana\\FindPaths\\Sanitize",
        "Hakana\\MustUse",
        "Hakana\\SecurityAnalysis\\IgnorePath",
        "Hakana\\SecurityAnalysis\\IgnorePathIfTrue",
        "Hakana\\SecurityAnalysis\\Sanitize",
        "Hakana\\SecurityAnalysis\\ShapeSource",
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
        "assert",
        "assertAll",
        "at",
        "class_exists",
        "coerce",
        "debug_backtrace",
        "dirname",
        "echo",
        "fromItems",
        "function_exists",
        "include",
        "isset",
        "keyExists",
        "microtime",
        "parent",
        "preg_replace",
        "preg_split",
        "range",
        "removeKey",
        "self",
        "static",
        "stdClass",
        "str_replace",
        "this",
        "toArray",
        "toDict",
        "trigger_error",
        "unset",
        "base64_decode",
        "basename",
        "date",
        "date_format",
        "file_get_contents",
        "hash_equals",
        "hash_hmac",
        "hex2bin",
        "idx",
        "in_array",
        "json_encode",
        "ltrim",
        "mb_strlen",
        "mb_strtolower",
        "mb_strtoupper",
        "md5",
        "mktime",
        "password_hash",
        "rand",
        "realpath",
        "rtrim",
        "sha1",
        "str_repeat",
        "strpad",
        "strtolower",
        "strtotime",
        "strtoupper",
        "trim",
        "utf8_encode",
        "vsprintf",
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
