use std::{
    collections::BTreeMap,
    hash::BuildHasherDefault,
    sync::{Arc, Mutex},
};

use indexmap::{IndexMap, IndexSet};
use rustc_hash::{self, FxHashMap, FxHasher};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct StrId(pub u32);

impl StrId {
    pub const EMPTY: StrId = StrId(0);
    pub const ANONYMOUS_FUNCTION: StrId = StrId(1);
    pub const ARIA_ATTRIBUTE: StrId = StrId(2);
    pub const DATA_ATTRIBUTE: StrId = StrId(3);
    pub const CODEGEN: StrId = StrId(4);
    pub const DOMDOCUMENT: StrId = StrId(5);
    pub const DATE_TIME: StrId = StrId(6);
    pub const DATE_TIME_IMMUTABLE: StrId = StrId(7);
    pub const ANY_ARRAY: StrId = StrId(8);
    pub const ASIO_JOIN: StrId = StrId(9);
    pub const ASYNC_ITERATOR: StrId = StrId(10);
    pub const ASYNC_KEYED_ITERATOR: StrId = StrId(11);
    pub const AWAITABLE: StrId = StrId(12);
    pub const BUILTIN_ENUM: StrId = StrId(13);
    pub const BUILTIN_ENUM_CLASS: StrId = StrId(14);
    pub const CONTAINER: StrId = StrId(15);
    pub const ENUM_CLASS_LABEL: StrId = StrId(16);
    pub const FORMAT_STRING: StrId = StrId(17);
    pub const ITERATOR: StrId = StrId(18);
    pub const KEYED_CONTAINER: StrId = StrId(19);
    pub const KEYED_ITERATOR: StrId = StrId(20);
    pub const KEYED_TRAVERSABLE: StrId = StrId(21);
    pub const LIB_C_CONTAINS: StrId = StrId(22);
    pub const LIB_C_CONTAINS_KEY: StrId = StrId(23);
    pub const LIB_C_FIRSTX: StrId = StrId(24);
    pub const LIB_C_LASTX: StrId = StrId(25);
    pub const LIB_C_ONLYX: StrId = StrId(26);
    pub const LIB_DICT_CONTAINS: StrId = StrId(27);
    pub const LIB_DICT_CONTAINS_KEY: StrId = StrId(28);
    pub const LIB_MATH_INT32_MAX: StrId = StrId(29);
    pub const LIB_REGEX_PATTERN: StrId = StrId(30);
    pub const LIB_REGEX_MATCHES: StrId = StrId(31);
    pub const LIB_STR_FORMAT: StrId = StrId(32);
    pub const LIB_STR_JOIN: StrId = StrId(33);
    pub const LIB_STR_REPLACE: StrId = StrId(34);
    pub const LIB_STR_SLICE: StrId = StrId(35);
    pub const LIB_STR_SPLIT: StrId = StrId(36);
    pub const LIB_STR_STARTS_WITH: StrId = StrId(37);
    pub const LIB_STR_STRIP_SUFFIX: StrId = StrId(38);
    pub const LIB_STR_TRIM: StrId = StrId(39);
    pub const MEMBER_OF: StrId = StrId(40);
    pub const SHAPES: StrId = StrId(41);
    pub const TRAVERSABLE: StrId = StrId(42);
    pub const TYPE_STRUCTURE: StrId = StrId(43);
    pub const VECTOR: StrId = StrId(44);
    pub const GLOBAL_GET: StrId = StrId(45);
    pub const IDX_FN: StrId = StrId(46);
    pub const INVARIANT: StrId = StrId(47);
    pub const INVARIANT_VIOLATION: StrId = StrId(48);
    pub const SET_FRAME_METADATA: StrId = StrId(49);
    pub const TYPE_STRUCTURE_FN: StrId = StrId(50);
    pub const HAKANA_FIND_PATHS_SANITIZE: StrId = StrId(51);
    pub const HAKANA_MUST_USE: StrId = StrId(52);
    pub const HAKANA_SECURITY_ANALYSIS_IGNORE_PATH: StrId = StrId(53);
    pub const HAKANA_SECURITY_ANALYSIS_IGNORE_PATH_IF_TRUE: StrId = StrId(54);
    pub const HAKANA_SECURITY_ANALYSIS_SANITIZE: StrId = StrId(55);
    pub const HAKANA_SECURITY_ANALYSIS_SHAPE_SOURCE: StrId = StrId(56);
    pub const HAKANA_SECURITY_ANALYSIS_SOURCE: StrId = StrId(57);
    pub const HAKANA_SECURITY_ANALYSIS_SPECIALIZE_CALL: StrId = StrId(58);
    pub const HAKANA_SPECIAL_TYPES_LITERAL_STRING: StrId = StrId(59);
    pub const NUMBER_FORMATTER: StrId = StrId(60);
    pub const SIMPLE_XML_ELEMENT: StrId = StrId(61);
    pub const XHP_CHILD: StrId = StrId(62);
    pub const DIR_CONST: StrId = StrId(63);
    pub const DYNAMICALLY_CALLABLE: StrId = StrId(64);
    pub const ENTRY_POINT: StrId = StrId(65);
    pub const FILE_CONST: StrId = StrId(66);
    pub const FUNCTION_CONST: StrId = StrId(67);
    pub const PHP_INCOMPLETE_CLASS: StrId = StrId(68);
    pub const CONSTRUCT: StrId = StrId(69);
    pub const ASSERT: StrId = StrId(70);
    pub const ASSERT_ALL: StrId = StrId(71);
    pub const AT: StrId = StrId(72);
    pub const CLASS_EXISTS: StrId = StrId(73);
    pub const COERCE: StrId = StrId(74);
    pub const DEBUG_BACKTRACE: StrId = StrId(75);
    pub const DIRNAME: StrId = StrId(76);
    pub const ECHO: StrId = StrId(77);
    pub const FROM_ITEMS: StrId = StrId(78);
    pub const FUNCTION_EXISTS: StrId = StrId(79);
    pub const INCLUDE: StrId = StrId(80);
    pub const ISSET: StrId = StrId(81);
    pub const KEY_EXISTS: StrId = StrId(82);
    pub const MICROTIME: StrId = StrId(83);
    pub const PARENT: StrId = StrId(84);
    pub const PREG_REPLACE: StrId = StrId(85);
    pub const PREG_SPLIT: StrId = StrId(86);
    pub const RANGE: StrId = StrId(87);
    pub const REMOVE_KEY: StrId = StrId(88);
    pub const SELF: StrId = StrId(89);
    pub const STATIC: StrId = StrId(90);
    pub const STD_CLASS: StrId = StrId(91);
    pub const STR_REPLACE: StrId = StrId(92);
    pub const THIS: StrId = StrId(93);
    pub const TO_ARRAY: StrId = StrId(94);
    pub const TO_DICT: StrId = StrId(95);
    pub const TRIGGER_ERROR: StrId = StrId(96);
    pub const UNSET: StrId = StrId(97);
    pub const BASE64_DECODE: StrId = StrId(98);
    pub const BASENAME: StrId = StrId(99);
    pub const DATE: StrId = StrId(100);
    pub const DATE_FORMAT: StrId = StrId(101);
    pub const FILE_GET_CONTENTS: StrId = StrId(102);
    pub const HASH_EQUALS: StrId = StrId(103);
    pub const HASH_HMAC: StrId = StrId(104);
    pub const HEX2BIN: StrId = StrId(105);
    pub const IDX: StrId = StrId(106);
    pub const IN_ARRAY: StrId = StrId(107);
    pub const JSON_ENCODE: StrId = StrId(108);
    pub const LTRIM: StrId = StrId(109);
    pub const MB_STRLEN: StrId = StrId(110);
    pub const MB_STRTOLOWER: StrId = StrId(111);
    pub const MB_STRTOUPPER: StrId = StrId(112);
    pub const MD5: StrId = StrId(113);
    pub const MKTIME: StrId = StrId(114);
    pub const PASSWORD_HASH: StrId = StrId(115);
    pub const RAND: StrId = StrId(116);
    pub const REALPATH: StrId = StrId(117);
    pub const RTRIM: StrId = StrId(118);
    pub const SHA1: StrId = StrId(119);
    pub const STR_REPEAT: StrId = StrId(120);
    pub const STRPAD: StrId = StrId(121);
    pub const STRTOLOWER: StrId = StrId(122);
    pub const STRTOTIME: StrId = StrId(123);
    pub const STRTOUPPER: StrId = StrId(124);
    pub const TRIM: StrId = StrId(125);
    pub const UTF8_ENCODE: StrId = StrId(126);
    pub const VSPRINTF: StrId = StrId(127);

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Interner {
    map: IndexSet<String, BuildHasherDefault<FxHasher>>,
}

impl Default for Interner {
    fn default() -> Self {
        let mut interner = Interner {
            map: IndexSet::default(),
        };
        interner.intern("".to_string());
        interner.intern("<anonymous function>".to_string());
        interner.intern("<aria attribute>".to_string());
        interner.intern("<data attribute>".to_string());
        interner.intern("Codegen".to_string());
        interner.intern("DOMDocument".to_string());
        interner.intern("DateTime".to_string());
        interner.intern("DateTimeImmutable".to_string());
        interner.intern("HH\\AnyArray".to_string());
        interner.intern("HH\\Asio\\join".to_string());
        interner.intern("HH\\AsyncIterator".to_string());
        interner.intern("HH\\AsyncKeyedIterator".to_string());
        interner.intern("HH\\Awaitable".to_string());
        interner.intern("HH\\BuiltinEnum".to_string());
        interner.intern("HH\\BuiltinEnumClass".to_string());
        interner.intern("HH\\Container".to_string());
        interner.intern("HH\\EnumClass\\Label".to_string());
        interner.intern("HH\\FormatString".to_string());
        interner.intern("HH\\Iterator".to_string());
        interner.intern("HH\\KeyedContainer".to_string());
        interner.intern("HH\\KeyedIterator".to_string());
        interner.intern("HH\\KeyedTraversable".to_string());
        interner.intern("HH\\Lib\\C\\contains".to_string());
        interner.intern("HH\\Lib\\C\\contains_key".to_string());
        interner.intern("HH\\Lib\\C\\firstx".to_string());
        interner.intern("HH\\Lib\\C\\lastx".to_string());
        interner.intern("HH\\Lib\\C\\onlyx".to_string());
        interner.intern("HH\\Lib\\Dict\\contains".to_string());
        interner.intern("HH\\Lib\\Dict\\contains_key".to_string());
        interner.intern("HH\\Lib\\Math\\INT32_MAX".to_string());
        interner.intern("HH\\Lib\\Regex\\Pattern".to_string());
        interner.intern("HH\\Lib\\Regex\\matches".to_string());
        interner.intern("HH\\Lib\\Str\\format".to_string());
        interner.intern("HH\\Lib\\Str\\join".to_string());
        interner.intern("HH\\Lib\\Str\\replace".to_string());
        interner.intern("HH\\Lib\\Str\\slice".to_string());
        interner.intern("HH\\Lib\\Str\\split".to_string());
        interner.intern("HH\\Lib\\Str\\starts_with".to_string());
        interner.intern("HH\\Lib\\Str\\strip_suffix".to_string());
        interner.intern("HH\\Lib\\Str\\trim".to_string());
        interner.intern("HH\\MemberOf".to_string());
        interner.intern("HH\\Shapes".to_string());
        interner.intern("HH\\Traversable".to_string());
        interner.intern("HH\\TypeStructure".to_string());
        interner.intern("HH\\Vector".to_string());
        interner.intern("HH\\global_get".to_string());
        interner.intern("HH\\idx".to_string());
        interner.intern("HH\\invariant".to_string());
        interner.intern("HH\\invariant_violation".to_string());
        interner.intern("HH\\set_frame_metadata".to_string());
        interner.intern("HH\\type_structure".to_string());
        interner.intern("Hakana\\FindPaths\\Sanitize".to_string());
        interner.intern("Hakana\\MustUse".to_string());
        interner.intern("Hakana\\SecurityAnalysis\\IgnorePath".to_string());
        interner.intern("Hakana\\SecurityAnalysis\\IgnorePathIfTrue".to_string());
        interner.intern("Hakana\\SecurityAnalysis\\Sanitize".to_string());
        interner.intern("Hakana\\SecurityAnalysis\\ShapeSource".to_string());
        interner.intern("Hakana\\SecurityAnalysis\\Source".to_string());
        interner.intern("Hakana\\SecurityAnalysis\\SpecializeCall".to_string());
        interner.intern("Hakana\\SpecialTypes\\LiteralString".to_string());
        interner.intern("NumberFormatter".to_string());
        interner.intern("SimpleXMLElement".to_string());
        interner.intern("XHPChild".to_string());
        interner.intern("__DIR__".to_string());
        interner.intern("__DynamicallyCallable".to_string());
        interner.intern("__EntryPoint".to_string());
        interner.intern("__FILE__".to_string());
        interner.intern("__FUNCTION__".to_string());
        interner.intern("__PHP_Incomplete_Class".to_string());
        interner.intern("__construct".to_string());
        interner.intern("assert".to_string());
        interner.intern("assertAll".to_string());
        interner.intern("at".to_string());
        interner.intern("class_exists".to_string());
        interner.intern("coerce".to_string());
        interner.intern("debug_backtrace".to_string());
        interner.intern("dirname".to_string());
        interner.intern("echo".to_string());
        interner.intern("fromItems".to_string());
        interner.intern("function_exists".to_string());
        interner.intern("include".to_string());
        interner.intern("isset".to_string());
        interner.intern("keyExists".to_string());
        interner.intern("microtime".to_string());
        interner.intern("parent".to_string());
        interner.intern("preg_replace".to_string());
        interner.intern("preg_split".to_string());
        interner.intern("range".to_string());
        interner.intern("removeKey".to_string());
        interner.intern("self".to_string());
        interner.intern("static".to_string());
        interner.intern("stdClass".to_string());
        interner.intern("str_replace".to_string());
        interner.intern("this".to_string());
        interner.intern("toArray".to_string());
        interner.intern("toDict".to_string());
        interner.intern("trigger_error".to_string());
        interner.intern("unset".to_string());
        interner.intern("base64_decode".to_string());
        interner.intern("basename".to_string());
        interner.intern("date".to_string());
        interner.intern("date_format".to_string());
        interner.intern("file_get_contents".to_string());
        interner.intern("hash_equals".to_string());
        interner.intern("hash_hmac".to_string());
        interner.intern("hex2bin".to_string());
        interner.intern("idx".to_string());
        interner.intern("in_array".to_string());
        interner.intern("json_encode".to_string());
        interner.intern("ltrim".to_string());
        interner.intern("mb_strlen".to_string());
        interner.intern("mb_strtolower".to_string());
        interner.intern("mb_strtoupper".to_string());
        interner.intern("md5".to_string());
        interner.intern("mktime".to_string());
        interner.intern("password_hash".to_string());
        interner.intern("rand".to_string());
        interner.intern("realpath".to_string());
        interner.intern("rtrim".to_string());
        interner.intern("sha1".to_string());
        interner.intern("str_repeat".to_string());
        interner.intern("strpad".to_string());
        interner.intern("strtolower".to_string());
        interner.intern("strtotime".to_string());
        interner.intern("strtoupper".to_string());
        interner.intern("trim".to_string());
        interner.intern("utf8_encode".to_string());
        interner.intern("vsprintf".to_string());
        interner
    }
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
