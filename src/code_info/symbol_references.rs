use rustc_hash::{FxHashMap, FxHashSet};

use crate::codebase_info::symbols::Symbol;
use crate::function_context::{FunctionContext, FunctionLikeIdentifier};

#[derive(Debug, Clone)]
pub struct SymbolReferences {
    // A lookup table of all symbols (classes, functions, enums etc) that reference a classlike member
    // (class method, enum case, class property etc)
    symbol_references_to_members: FxHashMap<Symbol, FxHashSet<(Symbol, String)>>,

    // A lookup table of all symbols (classes, functions, enums etc) that reference another symbol
    symbol_references_to_symbols: FxHashMap<Symbol, FxHashSet<Symbol>>,

    // A lookup table of all classlike members that reference another classlike member
    classlike_member_references_to_members:
        FxHashMap<(Symbol, String), FxHashSet<(Symbol, String)>>,

    // A lookup table of all classlike members that reference another symbol
    classlike_member_references_to_symbols: FxHashMap<(Symbol, String), FxHashSet<Symbol>>,

    // A lookup table of all symbols (classes, functions, enums etc) that reference a classlike member
    // (class method, enum case, class property etc)
    symbol_references_to_overridden_members: FxHashMap<Symbol, FxHashSet<(Symbol, String)>>,

    // A lookup table of all classlike members that reference another classlike member
    classlike_member_references_to_overridden_members:
        FxHashMap<(Symbol, String), FxHashSet<(Symbol, String)>>,

    // A lookup table used for getting all the functions that reference a method's return value
    // This is used for dead code detection when we want to see what return values are unused
    functionlike_references_to_functionlike_returns:
        FxHashMap<FunctionLikeIdentifier, FxHashSet<FunctionLikeIdentifier>>,
}

impl SymbolReferences {
    pub fn new() -> Self {
        Self {
            symbol_references_to_members: FxHashMap::default(),
            symbol_references_to_symbols: FxHashMap::default(),
            classlike_member_references_to_members: FxHashMap::default(),
            classlike_member_references_to_symbols: FxHashMap::default(),
            functionlike_references_to_functionlike_returns: FxHashMap::default(),
            symbol_references_to_overridden_members: FxHashMap::default(),
            classlike_member_references_to_overridden_members: FxHashMap::default(),
        }
    }

    pub fn add_symbol_reference_to_class_member(
        &mut self,
        referencing_symbol: Symbol,
        class_member: (Symbol, String),
    ) {
        self.add_symbol_reference_to_symbol(referencing_symbol.clone(), class_member.0.clone());
        self.symbol_references_to_members
            .entry(referencing_symbol)
            .or_insert_with(FxHashSet::default)
            .insert(class_member);
    }

    pub fn add_symbol_reference_to_symbol(&mut self, referencing_symbol: Symbol, symbol: Symbol) {
        self.symbol_references_to_symbols
            .entry(referencing_symbol)
            .or_insert_with(FxHashSet::default)
            .insert(symbol);
    }

    pub fn add_class_member_reference_to_class_member(
        &mut self,
        referencing_class_member: (Symbol, String),
        class_member: (Symbol, String),
    ) {
        self.add_symbol_reference_to_symbol(
            referencing_class_member.0.clone(),
            class_member.0.clone(),
        );
        self.classlike_member_references_to_members
            .entry(referencing_class_member)
            .or_insert_with(FxHashSet::default)
            .insert(class_member);
    }

    pub fn add_class_member_reference_to_symbol(
        &mut self,
        referencing_class_member: (Symbol, String),
        symbol: Symbol,
    ) {
        self.add_symbol_reference_to_symbol(referencing_class_member.0.clone(), symbol.clone());

        self.classlike_member_references_to_symbols
            .entry(referencing_class_member)
            .or_insert_with(FxHashSet::default)
            .insert(symbol);
    }

    pub fn add_reference_to_class_member(
        &mut self,
        function_context: &FunctionContext,
        class_member: (Symbol, String),
    ) {
        if let Some(referencing_functionlike) = &function_context.calling_functionlike_id {
            match referencing_functionlike {
                FunctionLikeIdentifier::Function(function_name) => {
                    self.add_symbol_reference_to_class_member(function_name.clone(), class_member)
                }
                FunctionLikeIdentifier::Method(class_name, function_name) => self
                    .add_class_member_reference_to_class_member(
                        (class_name.clone(), function_name.clone()),
                        class_member,
                    ),
            }
        } else if let Some(calling_class) = &function_context.calling_class {
            self.add_symbol_reference_to_class_member(calling_class.clone(), class_member)
        }
    }

    pub fn add_reference_to_overridden_class_member(
        &mut self,
        function_context: &FunctionContext,
        class_member: (Symbol, String),
    ) {
        if let Some(referencing_functionlike) = &function_context.calling_functionlike_id {
            match referencing_functionlike {
                FunctionLikeIdentifier::Function(function_name) => {
                    self.symbol_references_to_overridden_members
                        .entry(function_name.clone())
                        .or_insert_with(FxHashSet::default)
                        .insert(class_member);
                }
                FunctionLikeIdentifier::Method(class_name, function_name) => {
                    self.classlike_member_references_to_overridden_members
                        .entry((class_name.clone(), function_name.clone()))
                        .or_insert_with(FxHashSet::default)
                        .insert(class_member);
                }
            }
        } else if let Some(calling_class) = &function_context.calling_class {
            self.symbol_references_to_overridden_members
                .entry(calling_class.clone())
                .or_insert_with(FxHashSet::default)
                .insert(class_member);
        }
    }

    pub fn add_reference_to_symbol(&mut self, function_context: &FunctionContext, symbol: Symbol) {
        if let Some(referencing_functionlike) = &function_context.calling_functionlike_id {
            match referencing_functionlike {
                FunctionLikeIdentifier::Function(function_name) => {
                    self.add_symbol_reference_to_symbol(function_name.clone(), symbol)
                }
                FunctionLikeIdentifier::Method(class_name, function_name) => self
                    .add_class_member_reference_to_symbol(
                        (class_name.clone(), function_name.clone()),
                        symbol,
                    ),
            }
        } else if let Some(calling_class) = &function_context.calling_class {
            self.add_symbol_reference_to_symbol(calling_class.clone(), symbol)
        }
    }

    pub fn add_reference_to_functionlike_return(
        &mut self,
        referencing_functionlike: FunctionLikeIdentifier,
        functionlike: FunctionLikeIdentifier,
    ) {
        self.functionlike_references_to_functionlike_returns
            .entry(referencing_functionlike)
            .or_insert_with(FxHashSet::default)
            .insert(functionlike);
    }

    pub fn extend(&mut self, other: Self) {
        for (k, v) in other.symbol_references_to_members {
            self.symbol_references_to_members
                .entry(k)
                .or_insert_with(FxHashSet::default)
                .extend(v);
        }

        for (k, v) in other.symbol_references_to_symbols {
            self.symbol_references_to_symbols
                .entry(k)
                .or_insert_with(FxHashSet::default)
                .extend(v);
        }

        for (k, v) in other.classlike_member_references_to_symbols {
            self.classlike_member_references_to_symbols
                .entry(k)
                .or_insert_with(FxHashSet::default)
                .extend(v);
        }

        for (k, v) in other.classlike_member_references_to_members {
            self.classlike_member_references_to_members
                .entry(k)
                .or_insert_with(FxHashSet::default)
                .extend(v);
        }

        for (k, v) in other.symbol_references_to_overridden_members {
            self.symbol_references_to_overridden_members
                .entry(k)
                .or_insert_with(FxHashSet::default)
                .extend(v);
        }

        for (k, v) in other.classlike_member_references_to_overridden_members {
            self.classlike_member_references_to_overridden_members
                .entry(k)
                .or_insert_with(FxHashSet::default)
                .extend(v);
        }
    }

    pub fn get_referenced_symbols(&self) -> FxHashSet<&Symbol> {
        let mut referenced_symbols = FxHashSet::default();

        for (_, symbol_references_to_symbols) in &self.symbol_references_to_symbols {
            referenced_symbols.extend(symbol_references_to_symbols);
        }

        referenced_symbols
    }

    pub fn get_referenced_class_members(&self) -> FxHashSet<&(Symbol, String)> {
        let mut referenced_class_members = FxHashSet::default();

        for (_, symbol_references_to_class_members) in &self.symbol_references_to_members {
            referenced_class_members.extend(symbol_references_to_class_members);
        }

        for (_, class_member_references_to_class_members) in
            &self.classlike_member_references_to_members
        {
            referenced_class_members.extend(class_member_references_to_class_members);
        }

        referenced_class_members
    }

    pub fn get_referenced_overridden_class_members(&self) -> FxHashSet<&(Symbol, String)> {
        let mut referenced_class_members = FxHashSet::default();

        for (_, symbol_references_to_class_members) in &self.symbol_references_to_overridden_members
        {
            referenced_class_members.extend(symbol_references_to_class_members);
        }

        for (_, class_member_references_to_class_members) in
            &self.classlike_member_references_to_overridden_members
        {
            referenced_class_members.extend(class_member_references_to_class_members);
        }

        referenced_class_members
    }
}
