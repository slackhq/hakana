pub mod functionlike_identifier;
pub mod method_identifier;

use rustc_hash::FxHashMap;

pub use functionlike_identifier::FunctionLikeIdentifier;

#[derive(Clone, Debug)]
pub struct FunctionContext {
    pub namespace: Option<String>,

    pub calling_class: Option<String>,

    pub aliased_namespaces: FxHashMap<String, String>,

    pub aliased_types: FxHashMap<String, String>,

    pub aliased_functions: FxHashMap<String, String>,

    pub aliased_constants: FxHashMap<String, String>,

    /**
     * Whether or not to do a deep analysis and collect initializations from private or final methods
     */
    pub collect_initializations: bool,

    /**
     * Whether or not to do a deep analysis and collect initializations from public non-final methods
     */
    pub collect_nonprivate_initializations: bool,

    /**
     * Stored to prevent re-analysing methods when checking for initialised properties
     *
     * @var array<string, bool>|null
     */
    pub initialized_methods: Option<FxHashMap<String, bool>>,

    pub is_static: bool,

    pub calling_functionlike_id: Option<FunctionLikeIdentifier>,
    pub pure: bool,

    pub mutation_free: bool,

    pub external_mutation_free: bool,
}

impl FunctionContext {
    pub fn new() -> Self {
        Self {
            namespace: None,
            calling_class: None,
            aliased_namespaces: FxHashMap::default(),
            aliased_types: FxHashMap::default(),
            aliased_functions: FxHashMap::default(),
            aliased_constants: FxHashMap::default(),

            pure: false,
            mutation_free: false,
            external_mutation_free: false,
            collect_initializations: false,
            collect_nonprivate_initializations: false,
            initialized_methods: None,
            is_static: false,
            calling_functionlike_id: None,
        }
    }
}
