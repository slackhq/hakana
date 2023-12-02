use hakana_reflection_info::{StrId, ThreadedInterner};
use rustc_hash::FxHashMap;

use oxidized::aast::NsKind;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NameResolutionContext {
    namespace_name: String,
    type_aliases: FxHashMap<StrId, StrId>,
    namespace_aliases: FxHashMap<StrId, StrId>,
    const_aliases: FxHashMap<StrId, StrId>,
    fun_aliases: FxHashMap<StrId, StrId>,
}

impl NameResolutionContext {
    pub(crate) fn new(interner: &mut ThreadedInterner) -> Self {
        Self {
            namespace_name: "".to_string(),
            type_aliases: get_aliased_classes(interner),
            namespace_aliases: get_aliased_namespaces(interner),
            const_aliases: FxHashMap::default(),
            fun_aliases: get_aliased_functions(interner),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NameContext<'a> {
    name_resolution_contexts: Vec<NameResolutionContext>,
    namespace_name: Option<String>,
    pub symbol_name: Option<StrId>,
    pub member_name: Option<StrId>,
    pub in_class_id: bool,
    pub in_function_id: bool,
    pub in_constant_id: bool,
    pub in_xhp_id: bool,
    pub in_member_id: bool,
    pub generic_params: Vec<&'a String>,
}

impl NameContext<'_> {
    pub fn new(interner: &mut ThreadedInterner) -> Self {
        Self {
            name_resolution_contexts: vec![NameResolutionContext::new(interner)],
            namespace_name: None,
            in_class_id: false,
            in_function_id: false,
            in_xhp_id: false,
            in_constant_id: false,
            in_member_id: false,
            symbol_name: None,
            member_name: None,
            generic_params: vec![],
        }
    }

    /**
     * Start a new namespace.
     *
     * This also resets the alias table.
     *
     * @param Name|null $namespace Null is the global namespace
     */
    pub fn start_namespace(&mut self, namespace_name: String) {
        let existing_context = self.name_resolution_contexts.last().unwrap().clone();

        self.name_resolution_contexts.push(NameResolutionContext {
            namespace_name: namespace_name.clone(),
            type_aliases: existing_context.type_aliases,
            namespace_aliases: existing_context.namespace_aliases,
            const_aliases: existing_context.const_aliases,
            fun_aliases: existing_context.fun_aliases,
        });

        self.namespace_name = Some(if let Some(existing_name) = &self.namespace_name {
            format!("{}\\{}", existing_name, namespace_name)
        } else {
            namespace_name
        });
    }

    pub fn end_namespace(&mut self) {
        if self.name_resolution_contexts.len() > 1 {
            self.name_resolution_contexts.pop();
            self.namespace_name = if self.name_resolution_contexts.len() > 1 {
                Some(
                    self.name_resolution_contexts[1..]
                        .iter()
                        .map(|c| c.namespace_name.clone())
                        .collect::<Vec<_>>()
                        .join(""),
                )
            } else {
                None
            };
        }
    }

    /**
     * Add an alias / import.
     *
     * @param Name   $name        Original name
     * @param string $aliasName   Aliased name
     * @param int    $type        One of Stmt\Use_::TYPE_*
     * @param array  $errorAttrs Attributes to use to report an error
     */
    pub fn add_alias(
        &mut self,
        interner: &mut ThreadedInterner,
        name: &str,
        alias_name: &String,
        alias_kind: &NsKind,
    ) {
        let current_context = self.name_resolution_contexts.last_mut().unwrap();

        let alias_name = interner.intern_str(&alias_name);
        let name = interner.intern_str(&name);

        match alias_kind {
            NsKind::NSClass => {
                current_context.type_aliases.insert(alias_name, name);
            }
            NsKind::NSClassAndNamespace => {
                current_context.type_aliases.insert(alias_name, name);
                current_context.namespace_aliases.insert(alias_name, name);
            }
            NsKind::NSNamespace => {
                current_context.namespace_aliases.insert(alias_name, name);
            }
            NsKind::NSConst => {
                current_context.const_aliases.insert(alias_name, name);
            }
            NsKind::NSFun => {
                current_context.fun_aliases.insert(alias_name, name);
            }
        };
    }

    /**
     * Get current namespace.
     *
     * @return null|Name Namespace (or null if global namespace)
     */
    pub fn get_namespace_name(&self) -> &Option<String> {
        &self.namespace_name
    }

    /**
     * Get resolved name.
     *
     * @param Name $name Name to resolve
     */
    pub fn get_resolved_name(
        &mut self,
        interner: &mut ThreadedInterner,
        name: &String,
        alias_kind: NsKind,
        uses: &mut Vec<(StrId, StrId)>,
    ) -> StrId {
        // fully qualified names are already resolved
        if name.starts_with("\\") {
            return interner.intern_str(&name[1..]);
        }

        // XHP names preceded by : are already resolved
        if name.starts_with(":") {
            return interner.intern_str(&name[1..].replace(":", "\\"));
        }

        match name.as_str() {
            "this"
            | "static"
            | "self"
            | "parent"
            | "_"
            | "__AcceptDisposable"
            | "__ConsistentConstruct"
            | "__Deprecated"
            | "__DynamicallyCallable"
            | "__DynamicallyConstructible"
            | "__Enforceable"
            | "__EntryPoint"
            | "__Explicit"
            | "__LateInit"
            | "__LSB"
            | "__Memoize"
            | "__MemoizeLSB"
            | "__MockClass"
            | "__Newable"
            | "__Override"
            | "__PHPStdLib"
            | "__ReturnDisposable"
            | "__Sealed"
            | "__Soft" => {
                return interner.intern_str(name);
            }
            _ => {}
        }

        if self.generic_params.contains(&name) {
            return interner.intern_str(name);
        }

        let resolved_name = self.resolve_alias(interner, &name, alias_kind, uses);

        // Try to resolve aliases
        if let Some(resolved_name) = resolved_name {
            return resolved_name;
        }

        match self.get_namespace_name() {
            None => interner.intern_str(name),
            Some(inner_name) => interner.intern(format!("{}\\{}", inner_name, name)),
        }
    }

    fn resolve_alias(
        &mut self,
        interner: &mut ThreadedInterner,
        name: &String,
        alias_kind: NsKind,
        uses: &mut Vec<(StrId, StrId)>,
    ) -> Option<StrId> {
        let existing_context = self.name_resolution_contexts.last().unwrap();

        let parts: Vec<&str> = name.split('\\').collect();
        let first_part = parts.first().unwrap();

        if parts.len() > 1 {
            let alias = if *first_part == "namespace" {
                return Some(interner.intern(format!(
                    "{}\\{}",
                    self.get_namespace_name().as_ref().unwrap(),
                    parts[1..].join("\\")
                )));
            } else {
                existing_context
                    .namespace_aliases
                    .get(&interner.intern_str(first_part))
            };

            // resolve aliases for qualified names, always against class alias table
            if let Some(alias) = alias {
                let mut str = String::new();
                str += interner.lookup(*alias);
                str += "\\";
                str += parts[1..].join("\\").as_str();
                let resolved_qualified_alias = interner.intern(str);

                uses.push((*alias, resolved_qualified_alias));

                return Some(resolved_qualified_alias);
            }
        } else {
            let first_part = interner.intern_str(first_part);

            let alias = match alias_kind {
                NsKind::NSClass | NsKind::NSClassAndNamespace => {
                    existing_context.type_aliases.get(&first_part)
                }
                NsKind::NSNamespace => existing_context.namespace_aliases.get(&first_part),
                NsKind::NSConst => existing_context.const_aliases.get(&first_part),
                NsKind::NSFun => existing_context.fun_aliases.get(&first_part),
            };

            if let Some(inner_alias) = alias {
                uses.push((first_part, *inner_alias));

                return Some(*inner_alias);
            }
        }

        None
    }

    pub fn is_reserved(name: &String) -> bool {
        let reserved_types = [
            "mixed",
            "vec",
            "dict",
            "keyset",
            "varray",
            "darray",
            "arraykey",
            "bool",
            "classname",
            "dynamic",
            "float",
            "int",
            "nothing",
            "noreturn",
            "num",
            "shape",
            "string",
            "this",
            "Tuples",
            "void",
            "nonnull",
        ];

        reserved_types.contains(&name.as_str())
    }
}

fn get_aliased_classes(interner: &mut ThreadedInterner) -> FxHashMap<StrId, StrId> {
    let reserved_classes = vec![
        "Collection",
        "ConstCollection",
        "ConstMap",
        "ConstSet",
        "ConstVector",
        "Generator",
        "HH\\AnyArray",
        "HH\\AsyncFunctionWaitHandle",
        "HH\\AsyncGenerator",
        "HH\\AsyncGeneratorWaitHandle",
        "HH\\AsyncIterator",
        "HH\\AsyncKeyedIterator",
        "HH\\Awaitable",
        "HH\\AwaitAllWaitHandle",
        "HH\\BuiltinAbstractEnumClass",
        "HH\\BuiltinEnum",
        "HH\\BuiltinEnumClass",
        "HH\\classname",
        "HH\\Collection",
        "HH\\ConditionWaitHandle",
        "HH\\Container",
        "HH\\darray",
        "HH\\dict",
        "HH\\EnumClass\\Label",
        "HH\\ExternalThreadEventWaitHandle",
        "HH\\FormatString",
        "HH\\IMemoizeParam",
        "HH\\ImmMap",
        "HH\\ImmSet",
        "HH\\ImmVector",
        "HH\\InvariantException",
        "HH\\Iterable",
        "HH\\Iterator",
        "HH\\KeyedContainer",
        "HH\\KeyedIterable",
        "HH\\KeyedIterator",
        "HH\\KeyedTraversable",
        "HH\\keyset",
        "HH\\Map",
        "HH\\MemberOf",
        "HH\\ObjprofObjectStats",
        "HH\\ObjprofPathsStats",
        "HH\\ObjprofStringStats",
        "HH\\Pair",
        "HH\\RescheduleWaitHandle",
        "HH\\ResumableWaitHandle",
        "HH\\Set",
        "HH\\Shapes",
        "HH\\SleepWaitHandle",
        "HH\\StaticWaitHandle",
        "HH\\supportdyn",
        "HH\\supportdynamic",
        "HH\\Traversable",
        "HH\\typename",
        "HH\\TypeStructure",
        "HH\\TypeStructureKind",
        "HH\\UNSAFESingletonMemoizeParam",
        "HH\\varray",
        "HH\\varray_or_darray",
        "HH\\vec",
        "HH\\vec_or_dict",
        "HH\\Vector",
        "HH\\WaitableWaitHandle",
        "HH\\XenonSample",
        "IAsyncDisposable",
        "IDisposable",
        "MutableMap",
        "MutableSet",
        "MutableVector",
        "Spliceable",
        "stdClass",
        "Stringish",
        "StringishObject",
        "Throwable",
        "XHPChild",
    ];

    reserved_classes
        .into_iter()
        .map(|k| {
            (
                interner.intern(k.split("\\").last().unwrap().to_string()),
                interner.intern(k.to_string()),
            )
        })
        .collect()
}

fn get_aliased_functions(interner: &mut ThreadedInterner) -> FxHashMap<StrId, StrId> {
    let reserved_functions = vec![
        "HH\\asio_get_current_context_idx",
        "HH\\asio_get_running_in_context",
        "HH\\asio_get_running",
        "HH\\class_meth",
        "HH\\darray",
        "HH\\dict",
        "HH\\fun",
        "HH\\heapgraph_create",
        "HH\\heapgraph_dfs_edges",
        "HH\\heapgraph_dfs_nodes",
        "HH\\heapgraph_edge",
        "HH\\heapgraph_foreach_edge",
        "HH\\heapgraph_foreach_node",
        "HH\\heapgraph_foreach_root",
        "HH\\heapgraph_node_in_edges",
        "HH\\heapgraph_node_out_edges",
        "HH\\heapgraph_node",
        "HH\\heapgraph_stats",
        "HH\\idx",
        "HH\\idx_readonly",
        "HH\\inst_meth",
        "HH\\invariant_callback_register",
        "HH\\invariant_violation",
        "HH\\invariant",
        "HH\\is_darray",
        "HH\\is_dict",
        "HH\\is_keyset",
        "HH\\is_varray",
        "HH\\is_vec",
        "HH\\keyset",
        "HH\\meth_caller",
        "HH\\objprof_get_data",
        "HH\\objprof_get_paths",
        "HH\\objprof_get_strings",
        "HH\\server_warmup_status",
        "HH\\thread_mark_stack",
        "HH\\thread_memory_stats",
        "HH\\type_structure",
        "HH\\varray",
        "HH\\vec",
        "HH\\xenon_get_data",
        "isset",
        "unset",
        "echo",
        "exit",
        "die",
    ];

    reserved_functions
        .into_iter()
        .map(|k| {
            (
                interner.intern(k.split("\\").last().unwrap().to_string()),
                interner.intern(k.to_string()),
            )
        })
        .collect()
}

// todo load this from .hhconfig
fn get_aliased_namespaces(interner: &mut ThreadedInterner) -> FxHashMap<StrId, StrId> {
    FxHashMap::from_iter([
        (
            interner.intern("Vec".to_string()),
            interner.intern("HH\\Lib\\Vec".to_string()),
        ),
        (
            interner.intern("Dict".to_string()),
            interner.intern("HH\\Lib\\Dict".to_string()),
        ),
        (
            interner.intern("Str".to_string()),
            interner.intern("HH\\Lib\\Str".to_string()),
        ),
        (
            interner.intern("C".to_string()),
            interner.intern("HH\\Lib\\C".to_string()),
        ),
        (
            interner.intern("Keyset".to_string()),
            interner.intern("HH\\Lib\\Keyset".to_string()),
        ),
        (
            interner.intern("Math".to_string()),
            interner.intern("HH\\Lib\\Math".to_string()),
        ),
        (
            interner.intern("Asio".to_string()),
            interner.intern("HH\\Asio".to_string()),
        ),
    ])
}
