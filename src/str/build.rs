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
        "$$",
        "$_GET",
        "$_REQUEST",
        "$_POST",
        "$_COOKIE",
        "<anonymous function>",
        "<aria attribute>",
        "<data attribute>",
        "Codegen",
        "DOMDocument",
        "DateTime",
        "DateTimeImmutable",
        "Error",
        "Exception",
        "Generator",
        "HH\\AnyArray",
        "HH\\Asio\\join",
        "HH\\AsyncFunctionWaitHandle",
        "HH\\AsyncGenerator",
        "HH\\AsyncGeneratorWaitHandle",
        "HH\\AsyncIterator",
        "HH\\AsyncKeyedIterator",
        "HH\\Awaitable",
        "HH\\BuiltinEnum",
        "HH\\BuiltinEnumClass",
        "HH\\Container",
        "HH\\EnumClass\\Label",
        "HH\\Facts\\enabled",
        "HH\\FIXME\\UNSAFE_CAST",
        "HH\\FormatString",
        "HH\\Iterator",
        "HH\\KeyedContainer",
        "HH\\KeyedIterator",
        "HH\\KeyedTraversable",
        "HH\\Lib\\_Private\\regex_match",
        "HH\\Lib\\_Private\\validate_offset",
        "HH\\Lib\\C\\any",
        "HH\\Lib\\C\\contains_key",
        "HH\\Lib\\C\\contains",
        "HH\\Lib\\C\\count",
        "HH\\Lib\\C\\every",
        "HH\\Lib\\C\\find_key",
        "HH\\Lib\\C\\find",
        "HH\\Lib\\C\\findx",
        "HH\\Lib\\C\\first_key",
        "HH\\Lib\\C\\first_keyx",
        "HH\\Lib\\C\\first",
        "HH\\Lib\\C\\firstx",
        "HH\\Lib\\C\\is_empty",
        "HH\\Lib\\C\\last_key",
        "HH\\Lib\\C\\last_keyx",
        "HH\\Lib\\C\\last",
        "HH\\Lib\\C\\lastx",
        "HH\\Lib\\C\\nfirst",
        "HH\\Lib\\C\\onlyx",
        "HH\\Lib\\C\\search",
        "HH\\Lib\\Dict\\associate",
        "HH\\Lib\\Dict\\chunk",
        "HH\\Lib\\Dict\\contains_key",
        "HH\\Lib\\Dict\\contains",
        "HH\\Lib\\Dict\\count_values",
        "HH\\Lib\\Dict\\diff_by_key",
        "HH\\Lib\\Dict\\drop",
        "HH\\Lib\\Dict\\equal",
        "HH\\Lib\\Dict\\fill_keys",
        "HH\\Lib\\Dict\\filter_async",
        "HH\\Lib\\Dict\\filter_keys",
        "HH\\Lib\\Dict\\filter_nulls",
        "HH\\Lib\\Dict\\filter_with_key",
        "HH\\Lib\\Dict\\filter",
        "HH\\Lib\\Dict\\flatten",
        "HH\\Lib\\Dict\\flip",
        "HH\\Lib\\Dict\\from_async",
        "HH\\Lib\\Dict\\from_entries",
        "HH\\Lib\\Dict\\from_keys_async",
        "HH\\Lib\\Dict\\from_keys",
        "HH\\Lib\\Dict\\map_async",
        "HH\\Lib\\Dict\\map_with_key_async",
        "HH\\Lib\\Dict\\map_with_key",
        "HH\\Lib\\Dict\\map",
        "HH\\Lib\\Dict\\merge",
        "HH\\Lib\\Dict\\reverse",
        "HH\\Lib\\Dict\\select_keys",
        "HH\\Lib\\Dict\\take",
        "HH\\Lib\\Dict\\unique",
        "HH\\Lib\\Keyset\\chunk",
        "HH\\Lib\\Keyset\\diff",
        "HH\\Lib\\Keyset\\drop",
        "HH\\Lib\\Keyset\\equal",
        "HH\\Lib\\Keyset\\filter_async",
        "HH\\Lib\\Keyset\\filter_nulls",
        "HH\\Lib\\Keyset\\filter",
        "HH\\Lib\\Keyset\\flatten",
        "HH\\Lib\\Keyset\\intersect",
        "HH\\Lib\\Keyset\\keys",
        "HH\\Lib\\Keyset\\map_async",
        "HH\\Lib\\Keyset\\map_with_key",
        "HH\\Lib\\Keyset\\map",
        "HH\\Lib\\Keyset\\take",
        "HH\\Lib\\Keyset\\union",
        "HH\\Lib\\Legacy_FIXME\\eq",
        "HH\\Lib\\Legacy_FIXME\\lt",
        "HH\\Lib\\Legacy_FIXME\\neq",
        "HH\\Lib\\Locale\\create",
        "HH\\Lib\\Math\\abs",
        "HH\\Lib\\Math\\almost_equals",
        "HH\\Lib\\Math\\base_convert",
        "HH\\Lib\\Math\\ceil",
        "HH\\Lib\\Math\\cos",
        "HH\\Lib\\Math\\exp",
        "HH\\Lib\\Math\\floor",
        "HH\\Lib\\Math\\from_base",
        "HH\\Lib\\Math\\int_div",
        "HH\\Lib\\Math\\INT32_MAX",
        "HH\\Lib\\Math\\is_nan",
        "HH\\Lib\\Math\\log",
        "HH\\Lib\\Math\\max_by",
        "HH\\Lib\\Math\\max",
        "HH\\Lib\\Math\\maxva",
        "HH\\Lib\\Math\\mean",
        "HH\\Lib\\Math\\median",
        "HH\\Lib\\Math\\min_by",
        "HH\\Lib\\Math\\min",
        "HH\\Lib\\Math\\minva",
        "HH\\Lib\\Math\\round",
        "HH\\Lib\\Math\\sin",
        "HH\\Lib\\Math\\sqrt",
        "HH\\Lib\\Math\\sum_float",
        "HH\\Lib\\Math\\sum",
        "HH\\Lib\\Math\\tan",
        "HH\\Lib\\Math\\to_base",
        "HH\\Lib\\Regex\\every_match",
        "HH\\Lib\\Regex\\first_match",
        "HH\\Lib\\Regex\\matches",
        "HH\\Lib\\Regex\\Pattern",
        "HH\\Lib\\Regex\\replace",
        "HH\\Lib\\Regex\\split",
        "HH\\Lib\\Str\\capitalize_words",
        "HH\\Lib\\Str\\capitalize",
        "HH\\Lib\\Str\\chunk",
        "HH\\Lib\\Str\\compare_ci",
        "HH\\Lib\\Str\\compare",
        "HH\\Lib\\Str\\contains_ci",
        "HH\\Lib\\Str\\contains",
        "HH\\Lib\\Str\\ends_with_ci",
        "HH\\Lib\\Str\\ends_with",
        "HH\\Lib\\Str\\format_number",
        "HH\\Lib\\Str\\format",
        "HH\\Lib\\Str\\is_empty",
        "HH\\Lib\\Str\\join",
        "HH\\Lib\\Str\\length_l",
        "HH\\Lib\\Str\\length",
        "HH\\Lib\\Str\\lowercase",
        "HH\\Lib\\Str\\pad_left",
        "HH\\Lib\\Str\\pad_right",
        "HH\\Lib\\Str\\repeat",
        "HH\\Lib\\Str\\replace_ci",
        "HH\\Lib\\Str\\replace_every",
        "HH\\Lib\\Str\\replace",
        "HH\\Lib\\Str\\reverse",
        "HH\\Lib\\Str\\search_ci",
        "HH\\Lib\\Str\\search_l",
        "HH\\Lib\\Str\\search_last_l",
        "HH\\Lib\\Str\\search_last",
        "HH\\Lib\\Str\\search",
        "HH\\Lib\\Str\\slice_l",
        "HH\\Lib\\Str\\slice",
        "HH\\Lib\\Str\\splice",
        "HH\\Lib\\Str\\split",
        "HH\\Lib\\Str\\starts_with_ci",
        "HH\\Lib\\Str\\starts_with",
        "HH\\Lib\\Str\\strip_prefix",
        "HH\\Lib\\Str\\strip_suffix",
        "HH\\Lib\\Str\\to_int",
        "HH\\Lib\\Str\\trim_left",
        "HH\\Lib\\Str\\trim_right",
        "HH\\Lib\\Str\\trim",
        "HH\\Lib\\Str\\uppercase",
        "HH\\Lib\\Vec\\cast_clear_legacy_array_mark",
        "HH\\Lib\\Vec\\chunk",
        "HH\\Lib\\Vec\\concat",
        "HH\\Lib\\Vec\\diff",
        "HH\\Lib\\Vec\\drop",
        "HH\\Lib\\Vec\\fill",
        "HH\\Lib\\Vec\\filter_async",
        "HH\\Lib\\Vec\\filter_nulls",
        "HH\\Lib\\Vec\\filter_with_key",
        "HH\\Lib\\Vec\\filter",
        "HH\\Lib\\Vec\\flatten",
        "HH\\Lib\\Vec\\from_async",
        "HH\\Lib\\Vec\\intersect",
        "HH\\Lib\\Vec\\keys",
        "HH\\Lib\\Vec\\map_async",
        "HH\\Lib\\Vec\\map_with_key",
        "HH\\Lib\\Vec\\map",
        "HH\\Lib\\Vec\\range",
        "HH\\Lib\\Vec\\reverse",
        "HH\\Lib\\Vec\\slice",
        "HH\\Lib\\Vec\\sort",
        "HH\\Lib\\Vec\\take",
        "HH\\Lib\\Vec\\unique",
        "HH\\Lib\\Vec\\zip",
        "HH\\MemberOf",
        "HH\\ReifiedGenerics\\get_classname",
        "HH\\ReifiedGenerics\\get_type_structure",
        "HH\\Shapes",
        "HH\\Traversable",
        "HH\\TypeStructure",
        "HH\\Vector",
        "HH\\class_meth_get_class",
        "HH\\class_meth_get_method",
        "HH\\darray",
        "HH\\dict",
        "HH\\ffp_parse_string_native",
        "HH\\fun_get_function",
        "HH\\global_get",
        "HH\\idx",
        "HH\\invariant",
        "HH\\invariant_violation",
        "HH\\is_any_array",
        "HH\\is_dict",
        "HH\\is_dict_or_darray",
        "HH\\is_fun",
        "HH\\is_php_array",
        "HH\\is_vec",
        "HH\\is_vec_or_varray",
        "HH\\keyset",
        "HH\\non_crypto_md5_lower",
        "HH\\non_crypto_md5_upper",
        "HH\\set_frame_metadata",
        "HH\\str_number_coercible",
        "HH\\str_to_numeric",
        "HH\\type_structure",
        "HH\\type_structure_for_alias",
        "HH\\varray",
        "HH\\vec",
        "Hakana\\BannedFunction",
        "Hakana\\CallsDbAsioJoin",
        "Hakana\\HasDbAsioJoin",
        "Hakana\\FindPaths\\Sanitize",
        "Hakana\\CallsService",
        "Hakana\\IndirectlyCallsService",
        "Hakana\\HasDbOperation",
        "Hakana\\IgnoreNoreturnCalls",
        "Hakana\\Immutable",
        "Hakana\\MustUse",
        "Hakana\\NotTestOnly",
        "Hakana\\RequestHandler",
        "Hakana\\SecurityAnalysis\\IgnorePath",
        "Hakana\\SecurityAnalysis\\IgnorePathIfTrue",
        "Hakana\\SecurityAnalysis\\RemoveTaintsWhenReturningTrue",
        "Hakana\\SecurityAnalysis\\Sanitize",
        "Hakana\\SecurityAnalysis\\ShapeSource",
        "Hakana\\SecurityAnalysis\\Sink",
        "Hakana\\SecurityAnalysis\\Source",
        "Hakana\\SecurityAnalysis\\SpecializeCall",
        "Hakana\\SpecialTypes\\LiteralString",
        "Hakana\\TestOnly",
        "MessageFormatter",
        "NumberFormatter",
        "ReflectionClass",
        "ReflectionFunction",
        "ReflectionTypeAlias",
        "SimpleXMLElement",
        "Throwable",
        "XHPChild",
        "__DIR__",
        "__DynamicallyCallable",
        "__EntryPoint",
        "__FILE__",
        "__FUNCTION__",
        "__Override",
        "__PHP_Incomplete_Class",
        "__Sealed",
        "__construct",
        "abs",
        "addcslashes",
        "addslashes",
        "array_combine",
        "array_key_exists",
        "array_keys",
        "array_merge",
        "array_push",
        "array_reverse",
        "array_shift",
        "array_slice",
        "array_unique",
        "array_unshift",
        "arsort",
        "asin",
        "asort",
        "assert",
        "assertAll",
        "at",
        "atan2",
        "base64_decode",
        "base64_encode",
        "basename",
        "bin2hex",
        "ceil",
        "chop",
        "chr",
        "chunk_split",
        "class_exists",
        "coerce",
        "convert_uudecode",
        "convert_uuencode",
        "count",
        "crc32",
        "ctype_alnum",
        "ctype_alpha",
        "ctype_digit",
        "ctype_lower",
        "ctype_punct",
        "ctype_space",
        "ctype_upper",
        "ctype_xdigit",
        "curl_error",
        "date",
        "date_format",
        "debug_backtrace",
        "decbin",
        "dechex",
        "deg2rad",
        "dirname",
        "echo",
        "escapeshellarg",
        "explode",
        "extension",
        "fb_serialize",
        "file_get_contents",
        "filename",
        "filter_var",
        "floatval",
        "floor",
        "fmod",
        "formatMessage",
        "fromItems",
        "function_exists",
        "get_class",
        "get_object_vars",
        "get_parent_class",
        "get_resource_type",
        "gethostname",
        "getrandmax",
        "gettype",
        "gzcompress",
        "gzdecode",
        "gzdeflate",
        "gzinflate",
        "gzuncompress",
        "hash",
        "hash_equals",
        "hash_hmac",
        "hex2bin",
        "hexdec",
        "highlight_string",
        "hphp_to_string",
        "htmlentities",
        "htmlentitydecode",
        "htmlspecialchars",
        "htmlspecialchars_decode",
        "http_build_query",
        "idx",
        "implode",
        "in_array",
        "include",
        "inet_ntop",
        "inet_pton",
        "intdiv",
        "interface_exists",
        "intval",
        "ip2long",
        "is_a",
        "is_bool",
        "is_callable",
        "is_callable_with_name",
        "is_finite",
        "is_float",
        "is_infinite",
        "is_int",
        "is_nan",
        "is_null",
        "is_numeric",
        "is_object",
        "is_resource",
        "is_scalar",
        "is_string",
        "is_subclass_of",
        "isset",
        "join",
        "json_decode",
        "json_decode_with_error",
        "json_encode",
        "keyExists",
        "krsort",
        "ksort",
        "lcfirst",
        "levenshtein",
        "log",
        "long2ip",
        "ltrim",
        "lz4_compress",
        "lz4_uncompress",
        "max",
        "mb_detect_encoding",
        "mb_list_encodings",
        "mb_strlen",
        "mb_strtolower",
        "mb_strtoupper",
        "md5",
        "method_exists",
        "microtime",
        "min",
        "mktime",
        "mt_getrandmax",
        "mysql_escape_string",
        "nl2br",
        "number_format",
        "ord",
        "pack",
        "parent",
        "password_hash",
        "pathinfo",
        "pow",
        "preg_filter",
        "preg_grep",
        "preg_match",
        "preg_match_all",
        "preg_match_all_with_matches",
        "preg_match_with_error",
        "preg_match_with_matches",
        "preg_match_with_matches_and_error",
        "preg_quote",
        "preg_replace",
        "preg_replace_with_count",
        "preg_split",
        "print_r",
        "print_r_pure",
        "printf",
        "quote_meta",
        "quoted_printable_decode",
        "quoted_printable_encode",
        "rad2deg",
        "rand",
        "range",
        "rawurldecode",
        "rawurlencode",
        "realpath",
        "removeKey",
        "round",
        "rsort",
        "rtrim",
        "self",
        "serialize",
        "sha1",
        "socket_strerror",
        "sort",
        "sprintf",
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
        "str_word_count",
        "strcasecmp",
        "strchr",
        "strcmp",
        "strcspn",
        "stream_get_meta_data",
        "strgetcsv",
        "strip_tags",
        "stripcslashes",
        "stripos",
        "stripslashes",
        "stristr",
        "strlen",
        "strnatcasecmp",
        "strnatcmp",
        "strncmp",
        "strpad",
        "strpbrk",
        "strpos",
        "strrchr",
        "strrev",
        "strrpos",
        "strspn",
        "strstr",
        "strtolower",
        "strtotime",
        "strtoupper",
        "strtr",
        "strval",
        "substr",
        "substr_compare",
        "substr_count",
        "substr_replace",
        "this",
        "toArray",
        "toDict",
        "trigger_error",
        "trim",
        "ucfirst",
        "ucwords",
        "unpack",
        "unset",
        "urldecode",
        "urlencode",
        "utf8_decode",
        "utf8_encode",
        "var_dump",
        "var_export",
        "version_compare",
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

    if input == "$$" {
        return "DOLLAR_DOLLAR".to_string();
    }

    if input == "HH\\type_structure" {
        return "TYPE_STRUCTURE_FN".to_string();
    }

    if input == "HH\\idx" {
        return "IDX_FN".to_string();
    }

    if input.starts_with("$_") {
        return "MAGIC_".to_string() + &input[2..input.len()];
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
