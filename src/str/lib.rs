use std::{
    collections::BTreeMap,
    hash::BuildHasherDefault,
    sync::{Arc, Mutex},
};

use indexmap::{IndexMap, IndexSet};
use rustc_hash::{self, FxHashMap, FxHasher};
use serde::{Deserialize, Serialize};

mod str_macro;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct StrId(pub u32);

interned_strings! {
    EMPTY, 0 => "",
    ANONYMOUS_FUNCTION, 1 => "<anonymous function>",
    ARIA_ATTRIBUTE, 2 => "<aria attribute>",
    DATA_ATTRIBUTE, 3 => "<data attribute>",
    CODEGEN, 4 => "Codegen",
    DOMDOCUMENT, 5 => "DOMDocument",
    DATE_TIME, 6 => "DateTime",
    DATE_TIME_IMMUTABLE, 7 => "DateTimeImmutable",
    ANY_ARRAY, 8 => "HH\\AnyArray",
    ASIO_JOIN, 9 => "HH\\Asio\\join",
    ASYNC_ITERATOR, 10 => "HH\\AsyncIterator",
    ASYNC_KEYED_ITERATOR, 11 => "HH\\AsyncKeyedIterator",
    AWAITABLE, 12 => "HH\\Awaitable",
    BUILTIN_ENUM, 13 => "HH\\BuiltinEnum",
    BUILTIN_ENUM_CLASS, 14 => "HH\\BuiltinEnumClass",
    CONTAINER, 15 => "HH\\Container",
    ENUM_CLASS_LABEL, 16 => "HH\\EnumClass\\Label",
    FORMAT_STRING, 17 => "HH\\FormatString",
    ITERATOR, 18 => "HH\\Iterator",
    KEYED_CONTAINER, 19 => "HH\\KeyedContainer",
    KEYED_ITERATOR, 20 => "HH\\KeyedIterator",
    KEYED_TRAVERSABLE, 21 => "HH\\KeyedTraversable",
    LIB_C_CONTAINS, 22 => "HH\\Lib\\C\\contains",
    LIB_C_CONTAINS_KEY, 23 => "HH\\Lib\\C\\contains_key",
    LIB_C_FIRSTX, 24 => "HH\\Lib\\C\\firstx",
    LIB_C_LASTX, 25 => "HH\\Lib\\C\\lastx",
    LIB_C_ONLYX, 26 => "HH\\Lib\\C\\onlyx",
    LIB_DICT_CONTAINS, 27 => "HH\\Lib\\Dict\\contains",
    LIB_DICT_CONTAINS_KEY, 28 => "HH\\Lib\\Dict\\contains_key",
    LIB_MATH_INT32_MAX, 29 => "HH\\Lib\\Math\\INT32_MAX",
    LIB_REGEX_PATTERN, 30 => "HH\\Lib\\Regex\\Pattern",
    LIB_REGEX_MATCHES, 31 => "HH\\Lib\\Regex\\matches",
    LIB_STR_FORMAT, 32 => "HH\\Lib\\Str\\format",
    LIB_STR_JOIN, 33 => "HH\\Lib\\Str\\join",
    LIB_STR_REPLACE, 34 => "HH\\Lib\\Str\\replace",
    LIB_STR_SLICE, 35 => "HH\\Lib\\Str\\slice",
    LIB_STR_SPLIT, 36 => "HH\\Lib\\Str\\split",
    LIB_STR_STARTS_WITH, 37 => "HH\\Lib\\Str\\starts_with",
    LIB_STR_STRIP_SUFFIX, 38 => "HH\\Lib\\Str\\strip_suffix",
    LIB_STR_TRIM, 39 => "HH\\Lib\\Str\\trim",
    MEMBER_OF, 40 => "HH\\MemberOf",
    SHAPES, 41 => "HH\\Shapes",
    TRAVERSABLE, 42 => "HH\\Traversable",
    TYPE_STRUCTURE, 43 => "HH\\TypeStructure",
    VECTOR, 44 => "HH\\Vector",
    GLOBAL_GET, 45 => "HH\\global_get",
    IDX_FN, 46 => "HH\\idx",
    INVARIANT, 47 => "HH\\invariant",
    INVARIANT_VIOLATION, 48 => "HH\\invariant_violation",
    SET_FRAME_METADATA, 49 => "HH\\set_frame_metadata",
    TYPE_STRUCTURE_FN, 50 => "HH\\type_structure",
    HAKANA_FIND_PATHS_SANITIZE, 51 => "Hakana\\FindPaths\\Sanitize",
    HAKANA_MUST_USE, 52 => "Hakana\\MustUse",
    HAKANA_SECURITY_ANALYSIS_IGNORE_PATH, 53 => "Hakana\\SecurityAnalysis\\IgnorePath",
    HAKANA_SECURITY_ANALYSIS_IGNORE_PATH_IF_TRUE, 54 => "Hakana\\SecurityAnalysis\\IgnorePathIfTrue",
    HAKANA_SECURITY_ANALYSIS_SANITIZE, 55 => "Hakana\\SecurityAnalysis\\Sanitize",
    HAKANA_SECURITY_ANALYSIS_SHAPE_SOURCE, 56 => "Hakana\\SecurityAnalysis\\ShapeSource",
    HAKANA_SECURITY_ANALYSIS_SOURCE, 57 => "Hakana\\SecurityAnalysis\\Source",
    HAKANA_SECURITY_ANALYSIS_SPECIALIZE_CALL, 58 => "Hakana\\SecurityAnalysis\\SpecializeCall",
    HAKANA_SPECIAL_TYPES_LITERAL_STRING, 59 => "Hakana\\SpecialTypes\\LiteralString",
    NUMBER_FORMATTER, 60 => "NumberFormatter",
    SIMPLE_XML_ELEMENT, 61 => "SimpleXMLElement",
    XHP_CHILD, 62 => "XHPChild",
    DIR_CONST, 63 => "__DIR__",
    DYNAMICALLY_CALLABLE, 64 => "__DynamicallyCallable",
    ENTRY_POINT, 65 => "__EntryPoint",
    FILE_CONST, 66 => "__FILE__",
    FUNCTION_CONST, 67 => "__FUNCTION__",
    PHP_INCOMPLETE_CLASS, 68 => "__PHP_Incomplete_Class",
    CONSTRUCT, 69 => "__construct",
    ASSERT, 70 => "assert",
    ASSERT_ALL, 71 => "assertAll",
    AT, 72 => "at",
    CLASS_EXISTS, 73 => "class_exists",
    COERCE, 74 => "coerce",
    DEBUG_BACKTRACE, 75 => "debug_backtrace",
    DIRNAME, 76 => "dirname",
    ECHO, 77 => "echo",
    FROM_ITEMS, 78 => "fromItems",
    FUNCTION_EXISTS, 79 => "function_exists",
    INCLUDE, 80 => "include",
    ISSET, 81 => "isset",
    KEY_EXISTS, 82 => "keyExists",
    MICROTIME, 83 => "microtime",
    PARENT, 84 => "parent",
    PREG_REPLACE, 85 => "preg_replace",
    PREG_SPLIT, 86 => "preg_split",
    RANGE, 87 => "range",
    REMOVE_KEY, 88 => "removeKey",
    SELF, 89 => "self",
    STATIC, 90 => "static",
    STD_CLASS, 91 => "stdClass",
    STR_REPLACE, 92 => "str_replace",
    THIS, 93 => "this",
    TO_ARRAY, 94 => "toArray",
    TO_DICT, 95 => "toDict",
    TRIGGER_ERROR, 96 => "trigger_error",
    UNSET, 97 => "unset",
    BASE64_DECODE, 98 => "base64_decode",
    BASENAME, 99 => "basename",
    DATE, 100 => "date",
    DATE_FORMAT, 101 => "date_format",
    FILE_GET_CONTENTS, 102 => "file_get_contents",
    HASH_EQUALS, 103 => "hash_equals",
    HASH_HMAC, 104 => "hash_hmac",
    HEX2BIN, 105 => "hex2bin",
    IDX, 106 => "idx",
    IN_ARRAY, 107 => "in_array",
    JSON_ENCODE, 108 => "json_encode",
    LTRIM, 109 => "ltrim",
    MB_STRLEN, 110 => "mb_strlen",
    MB_STRTOLOWER, 111 => "mb_strtolower",
    MB_STRTOUPPER, 112 => "mb_strtoupper",
    MD5, 113 => "md5",
    MKTIME, 114 => "mktime",
    PASSWORD_HASH, 115 => "password_hash",
    RAND, 116 => "rand",
    REALPATH, 117 => "realpath",
    RTRIM, 118 => "rtrim",
    SHA1, 119 => "sha1",
    STR_REPEAT, 120 => "str_repeat",
    STRPAD, 121 => "strpad",
    STRTOLOWER, 122 => "strtolower",
    STRTOTIME, 123 => "strtotime",
    STRTOUPPER, 124 => "strtoupper",
    TRIM, 125 => "trim",
    UTF8_ENCODE, 126 => "utf8_encode",
    VSPRINTF, 127 => "vsprintf",
}

impl StrId {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Interner {
    map: IndexSet<String, BuildHasherDefault<FxHasher>>,
}

impl Interner {
    /// Get the id corresponding to `path`.
    ///
    /// If `path` does not exists in `self`, returns [`None`].
    pub fn get(&self, path: &str) -> Option<StrId> {
        self.map.get_index_of(path).map(|i| StrId(i as u32))
    }

    /// Insert `path` in `self`.
    ///
    /// - If `path` already exists in `self`, returns its associated id;
    /// - Else, returns a newly allocated id.
    pub fn intern(&mut self, path: String) -> StrId {
        let (id, _added) = self.map.insert_full(path);
        assert!(id < u32::MAX as usize);
        StrId(id as u32)
    }

    /// Returns the path corresponding to `id`.
    ///
    /// # Panics
    ///
    /// Panics if `id` does not exists in `self`.
    pub fn lookup(&self, id: &StrId) -> &str {
        self.map.get_index(id.0 as usize).unwrap()
    }

    pub fn get_map(&self) -> FxHashMap<String, StrId> {
        self.map
            .iter()
            .enumerate()
            .map(|(k, v)| (v.clone(), StrId(k as u32)))
            .collect()
    }
}

#[derive(Debug)]
pub struct ThreadedInterner {
    map: IndexMap<String, StrId>,
    reverse_map: BTreeMap<StrId, usize>,
    pub parent: Arc<Mutex<Interner>>,
}

impl ThreadedInterner {
    pub fn new(interner: Arc<Mutex<Interner>>) -> Self {
        ThreadedInterner {
            map: IndexMap::default(),
            reverse_map: BTreeMap::new(),
            parent: interner.clone(),
        }
    }

    pub fn intern_str(&mut self, path: &str) -> StrId {
        if let Some(id) = self.map.get(path) {
            return *id;
        }

        let id;
        {
            id = self.parent.lock().unwrap().intern(path.to_string());
        }
        let index = self.map.insert_full(path.to_string(), id).0;
        self.reverse_map.insert(id, index);

        id
    }

    pub fn intern(&mut self, path: String) -> StrId {
        if let Some(id) = self.map.get(&path) {
            return *id;
        }

        let id;
        {
            id = self.parent.lock().unwrap().intern(path.clone());
        }
        let index = self.map.insert_full(path, id).0;
        self.reverse_map.insert(id, index);

        id
    }

    pub fn lookup(&self, id: StrId) -> &str {
        if let Some(entry) = self.map.get_index(*self.reverse_map.get(&id).unwrap()) {
            entry.0
        } else {
            panic!()
        }
    }
}
