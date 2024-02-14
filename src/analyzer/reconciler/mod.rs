pub mod assertion_reconciler;
pub mod macros;
pub mod negated_assertion_reconciler;
pub mod simple_assertion_reconciler;
pub mod simple_negated_assertion_reconciler;

use crate::{
    function_analysis_data::FunctionAnalysisData,
    scope_analyzer::ScopeAnalyzer,
    scope_context::{var_has_root, ScopeContext},
    statements_analyzer::StatementsAnalyzer,
};
use hakana_reflection_info::{
    assertion::Assertion,
    codebase_info::CodebaseInfo,
    data_flow::{graph::GraphKind, node::DataFlowNode, path::PathKind},
    functionlike_identifier::FunctionLikeIdentifier,
    issue::{Issue, IssueKind},
    t_atomic::{DictKey, TAtomic},
    t_union::TUnion,
    Interner, StrId,
};
use hakana_type::{
    add_union_type, get_mixed_any, get_null, get_value_param,
    type_expander::{self, StaticClassType, TypeExpansionOptions},
    wrap_atomic,
};
use lazy_static::lazy_static;
use oxidized::ast_defs::Pos;
use regex::Regex;
use rustc_hash::{FxHashMap, FxHashSet};
use std::{collections::BTreeMap, rc::Rc, sync::Arc};

pub(crate) fn reconcile_keyed_types(
    new_types: &BTreeMap<String, Vec<Vec<Assertion>>>,
    // types we can complain about
    mut active_new_types: BTreeMap<String, FxHashSet<usize>>,
    context: &mut ScopeContext,
    changed_var_ids: &mut FxHashSet<String>,
    referenced_var_ids: &FxHashSet<String>,
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    pos: &Pos,
    can_report_issues: bool,
    negated: bool,
    suppressed_issues: &FxHashMap<String, usize>,
) {
    if new_types.is_empty() {
        return;
    }

    let inside_loop = context.inside_loop;

    let old_new_types = new_types.clone();

    let mut new_types = new_types.clone();

    add_nested_assertions(&mut new_types, &mut active_new_types, context);

    let codebase = statements_analyzer.get_codebase();

    // we want to remove any
    let mut added_var_ids = FxHashSet::default();

    for (key, new_type_parts) in &new_types {
        if key.contains("::") && !key.contains('$') && !key.contains('[') {
            continue;
        }

        let mut has_negation = false;
        let mut has_isset = false;
        let mut has_inverted_isset = false;
        let mut has_falsyish = false;
        let mut has_count_check = false;
        let is_real = old_new_types
            .get(key)
            .unwrap_or(&Vec::new())
            .eq(new_type_parts);

        let mut is_equality = false;

        for new_type_part_parts in new_type_parts {
            for assertion in new_type_part_parts {
                if key == "hakana taints" {
                    match assertion {
                        Assertion::RemoveTaints(key, taints) => {
                            if let Some(existing_var_type) = context.vars_in_scope.get_mut(key) {
                                let new_parent_node = DataFlowNode::get_for_assignment(
                                    key.clone(),
                                    statements_analyzer.get_hpos(pos),
                                );

                                for old_parent_node in &existing_var_type.parent_nodes {
                                    analysis_data.data_flow_graph.add_path(
                                        old_parent_node,
                                        &new_parent_node,
                                        PathKind::Default,
                                        None,
                                        Some(taints.clone()),
                                    );
                                }

                                let mut existing_var_type_inner = (**existing_var_type).clone();

                                existing_var_type_inner.parent_nodes =
                                    FxHashSet::from_iter([new_parent_node.clone()]);

                                *existing_var_type = Rc::new(existing_var_type_inner);

                                analysis_data.data_flow_graph.add_node(new_parent_node);
                            }
                        }
                        Assertion::IgnoreTaints => {
                            context.allow_taints = false;
                        }
                        Assertion::DontIgnoreTaints => {
                            context.allow_taints = true;
                        }
                        _ => (),
                    }

                    continue;
                }

                if assertion.has_negation() {
                    has_negation = true;
                }

                has_isset = has_isset || assertion.has_isset();

                has_falsyish = has_falsyish || matches!(assertion, Assertion::Falsy);

                is_equality = is_equality || assertion.has_non_isset_equality();

                has_inverted_isset =
                    has_inverted_isset || matches!(assertion, Assertion::IsNotIsset);

                has_count_check =
                    has_count_check || matches!(assertion, Assertion::NonEmptyCountable(_));
            }
        }

        let did_type_exist = context.vars_in_scope.contains_key(key);

        let mut possibly_undefined = false;

        let mut result_type = if let Some(existing_type) = context.vars_in_scope.get(key) {
            Some((**existing_type).clone())
        } else {
            get_value_for_key(
                codebase,
                statements_analyzer.get_interner(),
                key.clone(),
                context,
                &mut added_var_ids,
                &new_types,
                has_isset,
                has_inverted_isset,
                inside_loop,
                &mut possibly_undefined,
                analysis_data,
            )
        };

        if let Some(maybe_result_type) = &result_type {
            if maybe_result_type.types.is_empty() {
                panic!();
            }
        }

        let before_adjustment = result_type.clone();

        for (i, new_type_part_parts) in new_type_parts.iter().enumerate() {
            let mut orred_type: Option<TUnion> = None;

            for assertion in new_type_part_parts {
                let mut result_type_candidate = assertion_reconciler::reconcile(
                    assertion,
                    result_type.as_ref(),
                    possibly_undefined,
                    Some(key),
                    statements_analyzer,
                    analysis_data,
                    inside_loop,
                    Some(pos),
                    &context.function_context.calling_functionlike_id,
                    can_report_issues
                        && if referenced_var_ids.contains(key) && active_new_types.contains_key(key)
                        {
                            active_new_types.get(key).unwrap().get(&i).is_some()
                        } else {
                            false
                        },
                    negated,
                    suppressed_issues,
                );

                if result_type_candidate.types.is_empty() {
                    result_type_candidate.types.push(TAtomic::TNothing);
                }

                orred_type = if let Some(orred_type) = orred_type {
                    Some(add_union_type(
                        result_type_candidate,
                        &orred_type,
                        codebase,
                        false,
                    ))
                } else {
                    Some(result_type_candidate.clone())
                };
            }

            result_type = orred_type;
        }

        let mut result_type = result_type.unwrap();

        if !did_type_exist && result_type.is_nothing() {
            continue;
        }

        let type_changed = if let Some(before_adjustment) = &before_adjustment {
            &result_type != before_adjustment
        } else {
            true
        };

        if let Some(before_adjustment) = &before_adjustment {
            if let GraphKind::WholeProgram(_) = &analysis_data.data_flow_graph.kind {
                let mut has_scalar_restriction = false;

                for new_type_part_parts in new_type_parts {
                    if new_type_part_parts.len() == 1 {
                        let assertion = &new_type_part_parts[0];

                        if let Assertion::IsType(t) | Assertion::IsEqual(t) = assertion {
                            if t.is_some_scalar() {
                                has_scalar_restriction = true;
                            }
                        }
                    }
                }

                if has_scalar_restriction {
                    let scalar_check_node = DataFlowNode::get_for_assignment(
                        key.clone(),
                        statements_analyzer.get_hpos(pos),
                    );

                    for parent_node in &before_adjustment.parent_nodes {
                        analysis_data.data_flow_graph.add_path(
                            parent_node,
                            &scalar_check_node,
                            PathKind::ScalarTypeGuard,
                            None,
                            None,
                        );
                    }

                    result_type.parent_nodes = FxHashSet::from_iter([scalar_check_node.clone()]);

                    analysis_data.data_flow_graph.add_node(scalar_check_node);
                } else {
                    let narrowed_symbol = if type_changed {
                        if result_type.is_single() {
                            if let TAtomic::TNamedObject { name, .. } = result_type.get_single() {
                                Some(name)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    if let Some(narrowed_symbol) = narrowed_symbol {
                        let narrowing_node = DataFlowNode::get_for_assignment(
                            key.clone()
                                + " narrowed to "
                                + statements_analyzer.get_interner().lookup(narrowed_symbol),
                            statements_analyzer.get_hpos(pos),
                        );

                        for parent_node in &before_adjustment.parent_nodes {
                            analysis_data.data_flow_graph.add_path(
                                parent_node,
                                &narrowing_node,
                                PathKind::RefineSymbol(*narrowed_symbol),
                                None,
                                None,
                            );
                        }

                        result_type.parent_nodes = FxHashSet::from_iter([narrowing_node.clone()]);

                        analysis_data.data_flow_graph.add_node(narrowing_node);
                    } else {
                        result_type.parent_nodes = before_adjustment.parent_nodes.clone();
                    }
                }
            } else {
                result_type.parent_nodes = before_adjustment.parent_nodes.clone();
            }
        }

        if key.ends_with(']')
            && (type_changed || !did_type_exist)
            && !has_inverted_isset
            && !is_equality
        {
            let key_parts = break_up_path_into_parts(key);

            adjust_array_type(key_parts, context, changed_var_ids, &result_type);
        }

        if type_changed {
            changed_var_ids.insert(key.clone());

            if key != "$this" && !key.ends_with(']') {
                let mut removable_keys = Vec::new();
                for (new_key, _) in context.vars_in_scope.iter() {
                    if new_key.eq(key) {
                        continue;
                    }

                    if is_real && !new_types.contains_key(new_key) && var_has_root(new_key, key) {
                        removable_keys.push(new_key.clone());
                    }
                }

                for new_key in removable_keys {
                    context.vars_in_scope.remove(&new_key);
                }
            }
        } else if !has_negation && !has_falsyish && !has_isset {
            changed_var_ids.insert(key.clone());
        }

        context
            .vars_in_scope
            .insert(key.clone(), Rc::new(result_type));
    }

    context
        .vars_in_scope
        .retain(|var_id, _| !added_var_ids.contains(var_id));
}

fn adjust_array_type(
    mut key_parts: Vec<String>,
    context: &mut ScopeContext,
    changed_var_ids: &mut FxHashSet<String>,
    result_type: &TUnion,
) {
    key_parts.pop();
    let array_key = key_parts.pop().unwrap();
    key_parts.pop();

    if array_key.starts_with('$') {
        return;
    }

    let mut has_string_offset = false;

    let arraykey_offset = if array_key.starts_with('\'') || array_key.starts_with('\"') {
        has_string_offset = true;
        array_key[1..(array_key.len() - 1)].to_string()
    } else {
        array_key.clone()
    };

    let base_key = key_parts.join("");

    let mut existing_type = if let Some(existing_type) = context.vars_in_scope.get(&base_key) {
        (**existing_type).clone()
    } else {
        return;
    };

    for base_atomic_type in existing_type.types.iter_mut() {
        if let TAtomic::TTypeAlias {
            as_type: Some(as_type),
            ..
        } = base_atomic_type
        {
            *base_atomic_type = as_type.get_single().clone();
        }

        match base_atomic_type {
            TAtomic::TDict {
                ref mut known_items,
                ..
            } => {
                let dictkey = if has_string_offset {
                    DictKey::String(arraykey_offset.clone())
                } else if let Ok(arraykey_value) = arraykey_offset.parse::<u64>() {
                    DictKey::Int(arraykey_value)
                } else {
                    println!("bad int key {}", arraykey_offset);
                    continue;
                };

                if let Some(known_items) = known_items {
                    known_items.insert(dictkey, (false, Arc::new(result_type.clone())));
                } else {
                    *known_items = Some(BTreeMap::from([(
                        dictkey,
                        (false, Arc::new(result_type.clone())),
                    )]));
                }
            }
            TAtomic::TVec {
                ref mut known_items,
                ..
            } => {
                if let Ok(arraykey_offset) = arraykey_offset.parse::<usize>() {
                    if let Some(known_items) = known_items {
                        known_items.insert(arraykey_offset, (false, result_type.clone()));
                    } else {
                        *known_items = Some(BTreeMap::from([(
                            arraykey_offset,
                            (false, result_type.clone()),
                        )]));
                    }
                }
            }
            _ => {
                continue;
            }
        }

        changed_var_ids.insert(format!("{}[{}]", base_key, array_key.clone()));

        if let Some(last_part) = key_parts.last() {
            if last_part == "]" {
                adjust_array_type(
                    key_parts.clone(),
                    context,
                    changed_var_ids,
                    &wrap_atomic(base_atomic_type.clone()),
                );
            }
        }
    }

    context
        .vars_in_scope
        .insert(base_key, Rc::new(existing_type));
}

fn add_nested_assertions(
    new_types: &mut BTreeMap<String, Vec<Vec<Assertion>>>,
    active_new_types: &mut BTreeMap<String, FxHashSet<usize>>,
    context: &mut ScopeContext,
) {
    lazy_static! {
        static ref INTEGER_REGEX: Regex = Regex::new("^[0-9]+$").unwrap();
    }

    let mut keys_to_remove = vec![];

    'outer: for (nk, new_type) in new_types.clone() {
        if (nk.contains('[') || nk.contains("->"))
            && (new_type[0][0] == Assertion::IsEqualIsset || new_type[0][0] == Assertion::IsIsset)
        {
            let mut key_parts = break_up_path_into_parts(&nk);
            key_parts.reverse();

            let mut nesting = 0;

            let mut base_key = key_parts.pop().unwrap();

            if !&base_key.starts_with('$')
                && key_parts.len() > 2
                && key_parts.last().unwrap() == "::$"
            {
                base_key += key_parts.pop().unwrap().as_str();
                base_key += key_parts.pop().unwrap().as_str();
            }

            let base_key_set = if let Some(base_key_type) = context.vars_in_scope.get(&base_key) {
                !base_key_type.is_nullable()
            } else {
                false
            };

            if !base_key_set {
                if !new_types.contains_key(&base_key) {
                    new_types.insert(base_key.clone(), vec![vec![Assertion::IsEqualIsset]]);
                } else {
                    let mut existing_entry = new_types.get(&base_key).unwrap().clone();
                    existing_entry.push(vec![Assertion::IsEqualIsset]);
                    new_types.insert(base_key.clone(), existing_entry);
                }
            }

            while let Some(divider) = key_parts.pop() {
                if divider == "[" {
                    let array_key = key_parts.pop().unwrap();
                    key_parts.pop();

                    let new_base_key = base_key.clone() + "[" + array_key.as_str() + "]";

                    let entry = new_types.entry(base_key.clone()).or_default();

                    let new_key = if array_key.starts_with('\'') {
                        Some(DictKey::String(
                            array_key[1..(array_key.len() - 1)].to_string(),
                        ))
                    } else if array_key.starts_with('$') {
                        None
                    } else if let Ok(arraykey_value) = array_key.parse::<u64>() {
                        Some(DictKey::Int(arraykey_value))
                    } else {
                        println!("bad int key {}", array_key);
                        panic!()
                    };

                    if let Some(new_key) = new_key {
                        entry.push(vec![Assertion::HasNonnullEntryForKey(new_key)]);

                        if key_parts.is_empty() {
                            keys_to_remove.push(nk.clone());

                            if nesting == 0
                                && base_key_set
                                && active_new_types.remove(&nk).is_some()
                            {
                                active_new_types
                                    .entry(base_key.clone())
                                    .or_default()
                                    .insert(entry.len() - 1);
                            }

                            break 'outer;
                        }
                    } else {
                        entry.push(vec![if array_key.contains('\'') {
                            Assertion::HasStringArrayAccess
                        } else {
                            Assertion::HasIntOrStringArrayAccess
                        }]);
                    }

                    base_key = new_base_key;
                    nesting += 1;
                    continue;
                }

                if divider == "->" {
                    let property_name = key_parts.pop().unwrap();

                    let new_base_key = base_key.clone() + "->" + property_name.as_str();

                    if !new_types.contains_key(&base_key) {
                        new_types.insert(base_key.clone(), vec![vec![Assertion::IsIsset]]);
                    }

                    base_key = new_base_key;
                } else {
                    break;
                }

                if key_parts.is_empty() {
                    break;
                }
            }
        }
    }

    new_types.retain(|k, _| !keys_to_remove.contains(k));
}

fn break_up_path_into_parts(path: &str) -> Vec<String> {
    let chars: Vec<char> = path.chars().collect();

    let mut string_char: Option<char> = None;

    let mut escape_char = false;
    let mut brackets = 0;

    let mut parts = BTreeMap::new();
    parts.insert(0, "".to_string());
    let mut parts_offset = 0;

    let mut i = 0;
    let char_count = chars.len();

    while i < char_count {
        let ichar = *chars.get(i).unwrap();

        if let Some(string_char_inner) = string_char {
            if ichar == string_char_inner && !escape_char {
                string_char = None;
            }

            if ichar == '\\' {
                escape_char = !escape_char;
            }

            parts.insert(
                parts_offset,
                parts.get(&parts_offset).unwrap().clone() + ichar.to_string().as_str(),
            );

            i += 1;
            continue;
        }

        match ichar {
            '[' | ']' => {
                parts_offset += 1;
                parts.insert(parts_offset, ichar.to_string());
                parts_offset += 1;

                brackets += if ichar == '[' { 1 } else { -1 };

                i += 1;
                continue;
            }

            '\'' | '"' => {
                parts.entry(parts_offset).or_insert_with(|| "".to_string());
                parts.insert(
                    parts_offset,
                    parts.get(&parts_offset).unwrap().clone() + ichar.to_string().as_str(),
                );
                string_char = Some(ichar);

                i += 1;
                continue;
            }

            ':' => {
                if brackets == 0
                    && i < char_count - 2
                    && *chars.get(i + 1).unwrap() == ':'
                    && *chars.get(i + 2).unwrap() == '$'
                {
                    parts_offset += 1;
                    parts.insert(parts_offset, "::$".to_string());
                    parts_offset += 1;

                    i += 3;
                    continue;
                }
            }

            '-' => {
                if brackets == 0 && i < char_count - 1 && *chars.get(i + 1).unwrap() == '>' {
                    parts_offset += 1;
                    parts.insert(parts_offset, "->".to_string());
                    parts_offset += 1;

                    i += 2;
                    continue;
                }
            }

            _ => {}
        }

        parts.entry(parts_offset).or_insert_with(|| "".to_string());

        parts.insert(
            parts_offset,
            parts.get(&parts_offset).unwrap().clone() + ichar.to_string().as_str(),
        );

        i += 1;
    }

    parts.values().cloned().collect()
}

fn get_value_for_key(
    codebase: &CodebaseInfo,
    interner: &Interner,
    key: String,
    context: &mut ScopeContext,
    added_var_ids: &mut FxHashSet<String>,
    new_assertions: &BTreeMap<String, Vec<Vec<Assertion>>>,
    has_isset: bool,
    has_inverted_isset: bool,
    inside_loop: bool,
    possibly_undefined: &mut bool,
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    lazy_static! {
        static ref INTEGER_REGEX: Regex = Regex::new("^[0-9]+$").unwrap();
    }

    let mut key_parts = break_up_path_into_parts(&key);

    if key_parts.len() == 1 {
        if let Some(t) = context.vars_in_scope.get(&key) {
            return Some((**t).clone());
        }

        return None;
    }

    key_parts.reverse();

    let mut base_key = key_parts.pop().unwrap();

    if !base_key.starts_with('$')
        && key_parts.len() > 2
        && key_parts.last().unwrap().starts_with("::$")
    {
        base_key += key_parts.pop().unwrap().as_str();
        base_key += key_parts.pop().unwrap().as_str();
    }

    if !context.vars_in_scope.contains_key(&base_key) {
        if base_key.contains("::") {
            let base_key_parts = &base_key.split("::").collect::<Vec<&str>>();
            let fq_class_name = base_key_parts[0].to_string();
            let const_name = base_key_parts[1].to_string();

            let fq_class_name = &interner.get(fq_class_name.as_str()).unwrap();

            if !codebase.class_or_interface_exists(fq_class_name) {
                return None;
            }

            let class_constant = if let Some(const_name) = interner.get(&const_name) {
                codebase.get_class_constant_type(
                    fq_class_name,
                    false,
                    &const_name,
                    FxHashSet::default(),
                )
            } else {
                None
            };

            if let Some(class_constant) = class_constant {
                context
                    .vars_in_scope
                    .insert(base_key.clone(), Rc::new(class_constant));
            } else {
                return None;
            }
        } else {
            return None;
        }
    }

    while let Some(divider) = key_parts.pop() {
        if divider == "[" {
            let array_key = key_parts.pop().unwrap();
            key_parts.pop();

            let new_base_key = base_key.clone() + "[" + array_key.as_str() + "]";

            if !context.vars_in_scope.contains_key(&new_base_key) {
                let mut new_base_type: Option<TUnion> = None;

                let mut atomic_types = context.vars_in_scope.get(&base_key).unwrap().types.clone();

                atomic_types.reverse();

                while let Some(mut existing_key_type_part) = atomic_types.pop() {
                    if let TAtomic::TGenericParam { as_type, .. } = existing_key_type_part {
                        atomic_types.extend(as_type.types.clone());
                        continue;
                    }

                    if let TAtomic::TTypeAlias {
                        as_type: Some(as_type),
                        ..
                    } = existing_key_type_part
                    {
                        existing_key_type_part = as_type.get_single().clone();
                    }

                    let mut new_base_type_candidate;

                    if let TAtomic::TDict { known_items, .. } = &existing_key_type_part {
                        let known_item = if !array_key.starts_with('$') {
                            if let Some(known_items) = known_items {
                                let key_parts_key = array_key.replace('\'', "");
                                known_items.get(&DictKey::String(key_parts_key))
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Some(known_item) = known_item {
                            let known_item = known_item.clone();

                            new_base_type_candidate = (*known_item.1).clone();

                            if known_item.0 {
                                *possibly_undefined = true;
                            }
                        } else {
                            new_base_type_candidate =
                                get_value_param(&existing_key_type_part, codebase).unwrap();

                            if new_base_type_candidate.is_mixed()
                                && !has_isset
                                && !has_inverted_isset
                            {
                                return Some(new_base_type_candidate);
                            }

                            if (has_isset || has_inverted_isset)
                                && new_assertions.contains_key(&new_base_key)
                            {
                                if has_inverted_isset && new_base_key.eq(&key) {
                                    new_base_type_candidate = add_union_type(
                                        new_base_type_candidate,
                                        &get_null(),
                                        codebase,
                                        false,
                                    );
                                }

                                *possibly_undefined = true;
                            }
                        }
                    } else if let TAtomic::TVec { known_items, .. } = &existing_key_type_part {
                        let known_item = if INTEGER_REGEX.is_match(&array_key) {
                            if let Some(known_items) = known_items {
                                let key_parts_key = array_key.parse::<usize>().unwrap();
                                known_items.get(&key_parts_key)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Some(known_item) = known_item {
                            new_base_type_candidate = known_item.1.clone();

                            if known_item.0 {
                                *possibly_undefined = true;
                            }
                        } else {
                            new_base_type_candidate =
                                get_value_param(&existing_key_type_part, codebase).unwrap();

                            if (has_isset || has_inverted_isset)
                                && new_assertions.contains_key(&new_base_key)
                            {
                                if has_inverted_isset && new_base_key.eq(&key) {
                                    new_base_type_candidate = add_union_type(
                                        new_base_type_candidate,
                                        &get_null(),
                                        codebase,
                                        false,
                                    );
                                }

                                *possibly_undefined = true;
                            }
                        }
                    } else if matches!(
                        existing_key_type_part,
                        TAtomic::TString
                            | TAtomic::TLiteralString { .. }
                            | TAtomic::TStringWithFlags(..)
                    ) {
                        return Some(hakana_type::get_string());
                    } else if matches!(
                        existing_key_type_part,
                        TAtomic::TNothing | TAtomic::TMixedFromLoopIsset
                    ) {
                        return Some(hakana_type::get_mixed_maybe_from_loop(inside_loop));
                    } else if let TAtomic::TNamedObject {
                        name,
                        type_params: Some(type_params),
                        ..
                    } = &existing_key_type_part
                    {
                        match name {
                            &StrId::KEYED_CONTAINER | &StrId::CONTAINER => {
                                new_base_type_candidate = if name == &StrId::KEYED_CONTAINER {
                                    type_params[1].clone()
                                } else {
                                    type_params[0].clone()
                                };

                                if (has_isset || has_inverted_isset)
                                    && new_assertions.contains_key(&new_base_key)
                                {
                                    if has_inverted_isset && new_base_key.eq(&key) {
                                        new_base_type_candidate = add_union_type(
                                            new_base_type_candidate,
                                            &get_null(),
                                            codebase,
                                            false,
                                        );
                                    }

                                    *possibly_undefined = true;
                                }
                            }
                            _ => {
                                return Some(hakana_type::get_mixed_any());
                            }
                        }
                    } else {
                        return Some(hakana_type::get_mixed_any());
                    }

                    new_base_type = if let Some(new_base_type) = new_base_type {
                        Some(hakana_type::add_union_type(
                            new_base_type,
                            &new_base_type_candidate,
                            codebase,
                            false,
                        ))
                    } else {
                        Some(new_base_type_candidate.clone())
                    };

                    if !array_key.starts_with('$') {
                        added_var_ids.insert(new_base_key.clone());
                    }

                    context.vars_in_scope.insert(
                        new_base_key.clone(),
                        Rc::new(new_base_type.clone().unwrap()),
                    );
                }
            }

            base_key = new_base_key;
        } else if divider == "->" || divider == "::$" {
            let property_name = key_parts.pop().unwrap();

            let new_base_key = base_key.clone() + "->" + property_name.as_str();

            if !context.vars_in_scope.contains_key(&new_base_key) {
                let mut new_base_type: Option<TUnion> = None;

                let base_type = context.vars_in_scope.get(&base_key).unwrap();

                let mut atomic_types = base_type.types.clone();

                while let Some(existing_key_type_part) = atomic_types.pop() {
                    if let TAtomic::TGenericParam { as_type, .. } = existing_key_type_part {
                        atomic_types.extend(as_type.types.clone());
                        continue;
                    }

                    let class_property_type: TUnion;

                    if let TAtomic::TNull { .. } = existing_key_type_part {
                        class_property_type = get_null();
                    } else if let TAtomic::TMixed
                    | TAtomic::TMixedWithFlags(..)
                    | TAtomic::TGenericParam { .. }
                    | TAtomic::TObject { .. } = existing_key_type_part
                    {
                        class_property_type = get_mixed_any();
                    } else if let TAtomic::TNamedObject {
                        name: fq_class_name,
                        ..
                    } = existing_key_type_part
                    {
                        if fq_class_name == StrId::STDCLASS
                            || !codebase.class_or_interface_exists(&fq_class_name)
                        {
                            class_property_type = get_mixed_any();
                        } else if property_name.ends_with("()") {
                            // MAYBE TODO deal with memoisable method call memoisation
                            panic!();
                        } else {
                            let maybe_class_property_type = get_property_type(
                                codebase,
                                interner,
                                &fq_class_name,
                                &interner.get(&property_name)?,
                                analysis_data,
                            );

                            if let Some(maybe_class_property_type) = maybe_class_property_type {
                                class_property_type = maybe_class_property_type;
                            } else {
                                return None;
                            }
                        }
                    } else {
                        class_property_type = get_mixed_any();
                    }

                    new_base_type = if let Some(new_base_type) = new_base_type {
                        Some(hakana_type::add_union_type(
                            new_base_type,
                            &class_property_type,
                            codebase,
                            false,
                        ))
                    } else {
                        Some(class_property_type)
                    };

                    context.vars_in_scope.insert(
                        new_base_key.clone(),
                        Rc::new(new_base_type.clone().unwrap()),
                    );
                }
            }

            base_key = new_base_key;
        } else {
            return None;
        }
    }

    context.vars_in_scope.get(&base_key).map(|t| (**t).clone())
}

fn get_property_type(
    codebase: &CodebaseInfo,
    interner: &Interner,
    classlike_name: &StrId,
    property_name: &StrId,
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    if !codebase.property_exists(classlike_name, property_name) {
        return None;
    }

    let declaring_property_class =
        codebase.get_declaring_class_for_property(classlike_name, property_name);

    let declaring_property_class = declaring_property_class?;

    let class_property_type = codebase.get_property_type(classlike_name, property_name);

    if let Some(mut class_property_type) = class_property_type {
        type_expander::expand_union(
            codebase,
            &Some(interner),
            &mut class_property_type,
            &TypeExpansionOptions {
                self_class: Some(declaring_property_class),
                static_class_type: StaticClassType::Name(declaring_property_class),
                ..Default::default()
            },
            &mut analysis_data.data_flow_graph,
        );
        return Some(class_property_type);
    }

    Some(get_mixed_any())
}

pub(crate) fn trigger_issue_for_impossible(
    analysis_data: &mut FunctionAnalysisData,
    statements_analyzer: &StatementsAnalyzer,
    old_var_type_string: &String,
    key: &String,
    assertion: &Assertion,
    redundant: bool,
    negated: bool,
    pos: &Pos,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    _suppressed_issues: &FxHashMap<String, usize>,
) {
    let mut assertion_string = assertion.to_string(Some(statements_analyzer.get_interner()));
    let mut not_operator = assertion_string.starts_with('!');

    if not_operator {
        assertion_string = assertion_string[1..].to_string();
    }

    let mut redundant = redundant;

    if negated {
        not_operator = !not_operator;
        redundant = !redundant;
    }

    if redundant {
        if not_operator {
            if assertion_string == "falsy" {
                not_operator = false;
                assertion_string = "truthy".to_string();
            } else if assertion_string == "truthy" {
                not_operator = false;
                assertion_string = "falsy".to_string();
            }
        }

        analysis_data.maybe_add_issue(
            if not_operator {
                get_impossible_issue(
                    assertion,
                    &assertion_string,
                    key,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
                    old_var_type_string,
                )
            } else {
                get_redundant_issue(
                    assertion,
                    &assertion_string,
                    key,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
                    old_var_type_string,
                )
            },
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    } else {
        analysis_data.maybe_add_issue(
            if not_operator {
                get_redundant_issue(
                    assertion,
                    &assertion_string,
                    key,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
                    old_var_type_string,
                )
            } else {
                get_impossible_issue(
                    assertion,
                    &assertion_string,
                    key,
                    statements_analyzer,
                    pos,
                    calling_functionlike_id,
                    old_var_type_string,
                )
            },
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }
}

fn get_impossible_issue(
    assertion: &Assertion,
    assertion_string: &String,
    key: &String,
    statements_analyzer: &StatementsAnalyzer,
    pos: &Pos,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    old_var_type_string: &String,
) -> Issue {
    let old_var_type_string = if old_var_type_string.len() > 50 {
        if key.contains("tmp_coalesce_var") {
            "".to_string()
        } else {
            format!("of {} ", key)
        }
    } else {
        format!("{} ", old_var_type_string)
    };

    match assertion {
        Assertion::Truthy | Assertion::Falsy => Issue::new(
            IssueKind::ImpossibleTruthinessCheck,
            format!("Type {}is never {}", old_var_type_string, assertion_string),
            statements_analyzer.get_hpos(pos),
            calling_functionlike_id,
        ),
        Assertion::IsType(TAtomic::TNull) | Assertion::IsNotType(TAtomic::TNull) => Issue::new(
            IssueKind::ImpossibleNullTypeComparison,
            format!("{} is never null", key),
            statements_analyzer.get_hpos(pos),
            calling_functionlike_id,
        ),
        Assertion::HasArrayKey(key) | Assertion::DoesNotHaveArrayKey(key) => Issue::new(
            IssueKind::ImpossibleKeyCheck,
            format!(
                "Type {}never has key {}",
                old_var_type_string,
                key.to_string(Some(statements_analyzer.get_interner()))
            ),
            statements_analyzer.get_hpos(pos),
            calling_functionlike_id,
        ),
        Assertion::HasNonnullEntryForKey(dict_key) => Issue::new(
            IssueKind::ImpossibleNonnullEntryCheck,
            format!(
                "Type {}does not have a nonnull entry for {}",
                old_var_type_string,
                dict_key.to_string(Some(statements_analyzer.get_interner()))
            ),
            statements_analyzer.get_hpos(pos),
            calling_functionlike_id,
        ),
        _ => Issue::new(
            IssueKind::ImpossibleTypeComparison,
            format!("Type {}is never {}", old_var_type_string, &assertion_string),
            statements_analyzer.get_hpos(pos),
            calling_functionlike_id,
        ),
    }
}

fn get_redundant_issue(
    assertion: &Assertion,
    assertion_string: &String,
    key: &String,
    statements_analyzer: &StatementsAnalyzer,
    pos: &Pos,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    old_var_type_string: &String,
) -> Issue {
    let old_var_type_string = if old_var_type_string.len() > 50 {
        if key.contains("tmp_coalesce_var") {
            "".to_string()
        } else {
            format!("of {} ", key)
        }
    } else {
        format!("{} ", old_var_type_string)
    };

    match assertion {
        Assertion::IsIsset | Assertion::IsEqualIsset => Issue::new(
            IssueKind::RedundantIssetCheck,
            "Unnecessary isset check".to_string(),
            statements_analyzer.get_hpos(pos),
            calling_functionlike_id,
        ),
        Assertion::Truthy | Assertion::Falsy => Issue::new(
            IssueKind::RedundantTruthinessCheck,
            format!("Type {}is always {}", old_var_type_string, assertion_string),
            statements_analyzer.get_hpos(pos),
            calling_functionlike_id,
        ),
        Assertion::HasArrayKey(key) | Assertion::DoesNotHaveArrayKey(key) => Issue::new(
            IssueKind::RedundantKeyCheck,
            format!(
                "Type {}always has entry {}",
                old_var_type_string,
                key.to_string(Some(statements_analyzer.get_interner()))
            ),
            statements_analyzer.get_hpos(pos),
            calling_functionlike_id,
        ),
        Assertion::HasNonnullEntryForKey(key) => Issue::new(
            IssueKind::RedundantNonnullEntryCheck,
            format!(
                "Type {}always has entry {}",
                old_var_type_string,
                key.to_string(Some(statements_analyzer.get_interner()))
            ),
            statements_analyzer.get_hpos(pos),
            calling_functionlike_id,
        ),
        Assertion::IsType(TAtomic::TMixedWithFlags(_, _, _, true))
        | Assertion::IsNotType(TAtomic::TMixedWithFlags(_, _, _, true)) => Issue::new(
            IssueKind::RedundantNonnullTypeComparison,
            format!("{} is always nonnull", key),
            statements_analyzer.get_hpos(pos),
            calling_functionlike_id,
        ),
        _ => Issue::new(
            IssueKind::RedundantTypeComparison,
            format!("Type {}is always {}", old_var_type_string, assertion_string),
            statements_analyzer.get_hpos(pos),
            calling_functionlike_id,
        ),
    }
}
