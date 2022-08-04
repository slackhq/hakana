use crate::{
    scope_analyzer::ScopeAnalyzer,
    scope_context::{var_has_root, ScopeContext},
    statements_analyzer::StatementsAnalyzer,
    typed_ast::TastInfo,
};
use hakana_reflection_info::{
    assertion::Assertion,
    codebase_info::CodebaseInfo,
    data_flow::{node::DataFlowNode, path::PathKind},
    issue::{Issue, IssueKind},
    t_atomic::TAtomic,
    t_union::TUnion,
};
use hakana_type::{
    add_union_type, get_mixed_any, get_null, get_value_param,
    type_expander::{self, StaticClassType},
    wrap_atomic,
};
use lazy_static::lazy_static;
use oxidized::ast_defs::Pos;
use regex::Regex;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    rc::Rc,
    sync::Arc,
};

#[derive(PartialEq)]
pub(crate) enum ReconciliationStatus {
    Ok,
    Redundant,
    Empty,
}

pub(crate) fn reconcile_keyed_types(
    new_types: &BTreeMap<String, Vec<Vec<Assertion>>>,
    // types we can complain about
    active_new_types: BTreeMap<String, HashMap<usize, Vec<Assertion>>>,
    context: &mut ScopeContext,
    changed_var_ids: &mut HashSet<String>,
    referenced_var_ids: &HashSet<String>,
    statements_analyzer: &StatementsAnalyzer,
    tast_info: &mut TastInfo,
    pos: &Pos,
    can_report_issues: bool,
    negated: bool,
    suppressed_issues: &HashMap<String, usize>,
) {
    if new_types.is_empty() {
        return;
    }

    let inside_loop = context.inside_loop;

    let old_new_types = new_types.clone();

    let mut new_types = new_types.clone();

    add_nested_assertions(&mut new_types, context);

    let codebase = statements_analyzer.get_codebase();

    for (key, new_type_parts) in &new_types {
        if key.contains("::") && !key.contains("$") && !key.contains("[") {
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
        let mut is_equality = is_real;

        for new_type_part_parts in new_type_parts {
            for assertion in new_type_part_parts {
                if key == "hakana taints" {
                    match assertion {
                        Assertion::RemoveTaints(key, taints) => {
                            if let Some(existing_var_type) = context.vars_in_scope.get_mut(key) {
                                let new_parent_node = DataFlowNode::get_for_assignment(
                                    key.clone(),
                                    statements_analyzer.get_hpos(pos),
                                    None,
                                );

                                for (_, old_parent_node) in &existing_var_type.parent_nodes {
                                    tast_info.data_flow_graph.add_path(
                                        old_parent_node,
                                        &new_parent_node,
                                        PathKind::Default,
                                        HashSet::new(),
                                        taints.clone(),
                                    );
                                }

                                let mut existing_var_type_inner = (**existing_var_type).clone();

                                existing_var_type_inner.parent_nodes = HashMap::from([(
                                    new_parent_node.id.clone(),
                                    new_parent_node.clone(),
                                )]);

                                *existing_var_type = Rc::new(existing_var_type_inner);

                                tast_info.data_flow_graph.add_node(new_parent_node);
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
                key.clone(),
                context,
                &new_types,
                has_isset,
                has_inverted_isset,
                inside_loop,
                &mut possibly_undefined,
                tast_info,
            )
        };

        if let Some(maybe_result_type) = &result_type {
            if maybe_result_type.types.is_empty() {
                panic!();
            }
        }

        let before_adjustment = result_type.clone();

        let mut failed_reconciliation = ReconciliationStatus::Ok;

        let mut i = 0;

        for new_type_part_parts in new_type_parts {
            let mut orred_type: Option<TUnion> = None;

            for assertion in new_type_part_parts {
                let mut result_type_candidate = super::assertion_reconciler::reconcile(
                    assertion,
                    result_type.as_ref(),
                    possibly_undefined,
                    &Some(key.clone()),
                    statements_analyzer,
                    tast_info,
                    inside_loop,
                    Some(pos),
                    can_report_issues
                        && if referenced_var_ids.contains(key) && active_new_types.contains_key(key)
                        {
                            active_new_types
                                .get(key)
                                .unwrap()
                                .get(&(i as usize))
                                .is_some()
                        } else {
                            false
                        },
                    &mut failed_reconciliation,
                    negated,
                    suppressed_issues,
                );

                if result_type_candidate.types.is_empty() {
                    result_type_candidate
                        .types
                        .insert("nothing".to_string(), TAtomic::TNothing);
                }

                orred_type = if let Some(orred_type) = orred_type {
                    Some(add_union_type(
                        result_type_candidate,
                        &orred_type,
                        Some(codebase),
                        false,
                    ))
                } else {
                    Some(result_type_candidate.clone())
                };
            }

            i += 1;

            result_type = orred_type;
        }

        let mut result_type = result_type.unwrap();

        if !did_type_exist && result_type.is_nothing() {
            continue;
        }

        if let Some(before_adjustment) = &before_adjustment {
            result_type.parent_nodes = before_adjustment.parent_nodes.clone();
        }

        // TODO taint flow graph stuff
        // if (($statements_analyzer->data_flow_graph instanceof TaintFlowGraph

        let type_changed = if let Some(before_adjustment) = &before_adjustment {
            &result_type != before_adjustment
        } else {
            true
        };

        if type_changed || failed_reconciliation != ReconciliationStatus::Ok {
            changed_var_ids.insert(key.clone());

            if key.ends_with("]") && !has_inverted_isset && !is_equality {
                let key_parts = break_up_path_into_parts(key);

                adjust_array_type(key_parts, context, changed_var_ids, &result_type);
            } else if key != "$this" {
                let mut removable_keys = Vec::new();
                for (new_key, _) in context.vars_in_scope.iter() {
                    if new_key.eq(key) {
                        continue;
                    }

                    if is_real && !new_types.contains_key(new_key) {
                        if var_has_root(&new_key, key) {
                            removable_keys.push(new_key.clone());
                        }
                    }
                }

                for new_key in removable_keys {
                    context.vars_in_scope.remove(&new_key);
                }
            }
        } else if !has_negation && !has_falsyish && !has_isset {
            changed_var_ids.insert(key.clone());
        }

        if failed_reconciliation == ReconciliationStatus::Empty {
            result_type.failed_reconciliation = true;
        }

        context
            .vars_in_scope
            .insert(key.clone(), Rc::new(result_type));
    }
}

fn adjust_array_type(
    mut key_parts: Vec<String>,
    context: &mut ScopeContext,
    changed_var_ids: &mut HashSet<String>,
    result_type: &TUnion,
) {
    key_parts.pop();
    let array_key = key_parts.pop().unwrap();
    key_parts.pop();

    if array_key.starts_with("$") {
        return;
    }

    let arraykey_offset = if array_key.starts_with("'") || array_key.starts_with("\"") {
        array_key[1..][..1].to_string()
    } else {
        array_key.clone()
    };

    let base_key = key_parts.join("");

    let mut existing_type = if let Some(existing_type) = context.vars_in_scope.get(&base_key) {
        (**existing_type).clone()
    } else {
        return;
    };

    for (_, base_atomic_type) in existing_type.clone().types {
        let mut base_atomic_type = base_atomic_type;
        match base_atomic_type {
            TAtomic::TDict {
                ref mut known_items,
                ..
            } => {
                if let Some(known_items) = known_items {
                    known_items.insert(
                        arraykey_offset.clone(),
                        (false, Arc::new(result_type.clone())),
                    );
                } else {
                    *known_items = Some(BTreeMap::from([(
                        arraykey_offset.clone(),
                        (false, Arc::new(result_type.clone())),
                    )]));
                }
            }
            TAtomic::TVec {
                ref mut known_items,
                ..
            } => {
                let arraykey_offset = arraykey_offset.parse::<usize>().unwrap();
                if let Some(known_items) = known_items {
                    known_items.insert(arraykey_offset.clone(), (false, result_type.clone()));
                } else {
                    *known_items = Some(BTreeMap::from([(
                        arraykey_offset.clone(),
                        (false, result_type.clone()),
                    )]));
                }
            }
            _ => {
                continue;
            }
        }

        existing_type.add_type(base_atomic_type.clone());

        changed_var_ids.insert(format!("{}[{}]", base_key, array_key.clone()));

        if let Some(last_part) = key_parts.last() {
            if last_part == "]" {
                adjust_array_type(
                    key_parts.clone(),
                    context,
                    changed_var_ids,
                    &wrap_atomic(base_atomic_type),
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
    context: &mut ScopeContext,
) {
    lazy_static! {
        static ref INTEGER_REGEX: Regex = Regex::new("^[0-9]+$").unwrap();
    }

    for (nk, new_type) in new_types.clone() {
        if nk.contains("[") || nk.contains("->") {
            if new_type[0][0] == Assertion::IsEqualIsset || new_type[0][0] == Assertion::IsIsset {
                let mut key_parts = break_up_path_into_parts(&nk);
                key_parts.reverse();

                let mut base_key = key_parts.pop().unwrap();

                if !&base_key.starts_with("$")
                    && key_parts.len() > 2
                    && key_parts.last().unwrap() == "::$"
                {
                    base_key += key_parts.pop().unwrap().as_str();
                    base_key += key_parts.pop().unwrap().as_str();
                }

                if !context.vars_in_scope.contains_key(&base_key)
                    || context.vars_in_scope.get(&base_key).unwrap().is_nullable()
                {
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

                        let new_base_key = (&base_key).clone() + "[" + array_key.as_str() + "]";

                        new_types
                            .entry(base_key.clone())
                            .or_insert_with(Vec::new)
                            .push(vec![if array_key.contains("'") {
                                Assertion::HasStringArrayAccess
                            } else {
                                Assertion::HasIntOrStringArrayAccess
                            }]);

                        base_key = new_base_key;
                        continue;
                    }

                    if divider == "->" {
                        let property_name = key_parts.pop().unwrap();

                        let new_base_key = (&base_key).clone() + "->" + property_name.as_str();

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
    }
}

fn break_up_path_into_parts(path: &String) -> Vec<String> {
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
                if !parts.contains_key(&parts_offset) {
                    parts.insert(parts_offset, "".to_string());
                }
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

        if !parts.contains_key(&parts_offset) {
            parts.insert(parts_offset, "".to_string());
        }

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
    key: String,
    context: &mut ScopeContext,
    new_assertions: &BTreeMap<String, Vec<Vec<Assertion>>>,
    has_isset: bool,
    has_inverted_isset: bool,
    inside_loop: bool,
    possibly_undefined: &mut bool,
    tast_info: &mut TastInfo,
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

    if !base_key.starts_with("$")
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

            if !codebase.class_or_interface_exists(&fq_class_name) {
                return None;
            }

            let class_constant =
                codebase.get_class_constant_type(&fq_class_name, &const_name, HashSet::new());

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

            let new_base_key = (&base_key).clone() + "[" + array_key.as_str() + "]";

            if !context.vars_in_scope.contains_key(&new_base_key) {
                let mut new_base_type: Option<TUnion> = None;

                let mut atomic_types = (*context.vars_in_scope.get(&base_key).unwrap())
                    .types
                    .values()
                    .cloned()
                    .collect::<Vec<TAtomic>>();

                atomic_types.reverse();

                while let Some(existing_key_type_part) = atomic_types.pop() {
                    if let TAtomic::TTemplateParam { as_type, .. } = existing_key_type_part {
                        atomic_types
                            .extend(as_type.types.values().cloned().collect::<Vec<TAtomic>>());
                        continue;
                    }

                    let mut new_base_type_candidate;

                    if let TAtomic::TDict { known_items, .. } = &existing_key_type_part {
                        if matches!(known_items, Some(_)) && !array_key.starts_with("$") {
                            if let Some(known_items) = known_items {
                                let key_parts_key = array_key.replace("'", "");
                                if known_items.contains_key(&key_parts_key) {
                                    let cl = known_items[&key_parts_key].clone();

                                    new_base_type_candidate = (*cl.1).clone();
                                    if cl.0 {
                                        *possibly_undefined = true;
                                    }
                                } else {
                                    return None;
                                }
                            } else {
                                panic!();
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
                                    new_base_type_candidate.add_type(TAtomic::TNull);
                                }

                                *possibly_undefined = true;
                            }
                        }
                    } else if let TAtomic::TVec {
                        known_items,
                        type_param,
                        ..
                    } = &existing_key_type_part
                    {
                        if matches!(known_items, Some(_)) && INTEGER_REGEX.is_match(&array_key) {
                            let known_items = known_items.clone().unwrap();
                            let key_parts_key = array_key.parse::<usize>().unwrap();
                            if let Some((u, item)) = known_items.get(&key_parts_key) {
                                new_base_type_candidate = item.clone();
                                *possibly_undefined = *u;
                            } else if !type_param.is_nothing() {
                                new_base_type_candidate = type_param.clone();
                            } else {
                                return None;
                            }
                        } else {
                            new_base_type_candidate =
                                get_value_param(&existing_key_type_part, codebase).unwrap();

                            if (has_isset || has_inverted_isset)
                                && new_assertions.contains_key(&new_base_key)
                            {
                                if has_inverted_isset && new_base_key.eq(&key) {
                                    new_base_type_candidate.add_type(TAtomic::TNull);
                                }

                                *possibly_undefined = true;
                            }
                        }
                    } else if matches!(
                        existing_key_type_part,
                        TAtomic::TString { .. } | TAtomic::TLiteralString { .. }
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
                        if name == "HH\\KeyedContainer" || name == "HH\\Container" {
                            new_base_type_candidate = if name == "HH\\KeyedContainer" {
                                type_params[1].clone()
                            } else {
                                type_params[0].clone()
                            };

                            if (has_isset || has_inverted_isset)
                                && new_assertions.contains_key(&new_base_key)
                            {
                                if has_inverted_isset && new_base_key.eq(&key) {
                                    new_base_type_candidate.add_type(TAtomic::TNull);
                                }

                                *possibly_undefined = true;
                            }
                        } else {
                            return Some(hakana_type::get_mixed_any());
                        }
                    } else {
                        return Some(hakana_type::get_mixed_any());
                    }

                    new_base_type = if let Some(new_base_type) = new_base_type {
                        Some(hakana_type::add_union_type(
                            new_base_type,
                            &new_base_type_candidate,
                            Some(&codebase),
                            false,
                        ))
                    } else {
                        Some(new_base_type_candidate.clone())
                    };

                    context.vars_in_scope.insert(
                        new_base_key.clone(),
                        Rc::new(new_base_type.clone().unwrap()),
                    );
                }
            }

            base_key = new_base_key;
        } else if divider == "->" || divider == "::$" {
            let property_name = key_parts.pop().unwrap();

            let new_base_key = (&base_key).clone() + "->" + property_name.as_str();

            if !context.vars_in_scope.contains_key(&new_base_key) {
                let mut new_base_type: Option<TUnion> = None;

                let mut atomic_types = context
                    .vars_in_scope
                    .get(&base_key)
                    .unwrap()
                    .types
                    .values()
                    .cloned()
                    .collect::<Vec<TAtomic>>();

                while let Some(existing_key_type_part) = atomic_types.pop() {
                    if let TAtomic::TTemplateParam { as_type, .. } = existing_key_type_part {
                        atomic_types
                            .extend(as_type.types.values().cloned().collect::<Vec<TAtomic>>());
                        continue;
                    }

                    let class_property_type: TUnion;

                    if let TAtomic::TNull { .. } = existing_key_type_part {
                        class_property_type = get_null();
                    } else if let TAtomic::TMixed
                    | TAtomic::TMixedAny
                    | TAtomic::TTruthyMixed
                    | TAtomic::TFalsyMixed
                    | TAtomic::TNonnullMixed
                    | TAtomic::TTemplateParam { .. }
                    | TAtomic::TObject { .. } = existing_key_type_part
                    {
                        class_property_type = get_mixed_any();
                    } else if let TAtomic::TNamedObject {
                        name: fq_class_name,
                        ..
                    } = existing_key_type_part
                    {
                        if fq_class_name == "stdClass" {
                            class_property_type = get_mixed_any();
                        } else if !codebase.class_or_interface_exists(&fq_class_name) {
                            class_property_type = get_mixed_any();
                        } else {
                            if property_name.ends_with("()") {
                                // MAYBE TODO deal with memoisable method call memoisation
                                panic!();
                            } else {
                                let maybe_class_property_type = get_property_type(
                                    &codebase,
                                    &fq_class_name,
                                    &property_name,
                                    tast_info,
                                );

                                if let Some(maybe_class_property_type) = maybe_class_property_type {
                                    class_property_type = maybe_class_property_type;
                                } else {
                                    return None;
                                }
                            }
                        }
                    } else {
                        class_property_type = get_mixed_any();
                    }

                    new_base_type = if let Some(new_base_type) = new_base_type {
                        Some(hakana_type::add_union_type(
                            new_base_type,
                            &class_property_type,
                            Some(&codebase),
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

    if let Some(t) = context.vars_in_scope.get(&base_key) {
        return Some((**t).clone());
    } else {
        return None;
    }
}

fn get_property_type(
    codebase: &CodebaseInfo,
    classlike_name: &String,
    property_name: &String,
    tast_info: &mut TastInfo,
) -> Option<TUnion> {
    if !codebase.property_exists(classlike_name, property_name) {
        return None;
    }

    let declaring_property_class =
        codebase.get_declaring_class_for_property(classlike_name, property_name);

    let declaring_property_class = if let Some(declaring_property_class) = declaring_property_class
    {
        declaring_property_class
    } else {
        return None;
    };

    let class_property_type = codebase.get_property_type(classlike_name, property_name);

    if let Some(mut class_property_type) = class_property_type {
        type_expander::expand_union(
            codebase,
            &mut class_property_type,
            Some(declaring_property_class),
            &StaticClassType::Name(declaring_property_class),
            None,
            &mut tast_info.data_flow_graph,
            true,
            false,
            false,
            false,
            true,
        );
        return Some(class_property_type);
    }

    Some(get_mixed_any())
}

pub(crate) fn trigger_issue_for_impossible(
    tast_info: &mut TastInfo,
    statements_analyzer: &StatementsAnalyzer,
    old_var_type_string: &String,
    key: &String,
    assertion: &Assertion,
    redundant: bool,
    negated: bool,
    pos: &Pos,
    _suppressed_issues: &HashMap<String, usize>,
) {
    let mut assertion_string = assertion.to_string();
    let mut not_operator = assertion_string.starts_with("!");

    if not_operator {
        assertion_string = assertion_string[1..].to_string();
    }

    let mut redundant = redundant;

    if negated {
        not_operator = !not_operator;
        redundant = !redundant;
    }

    if redundant {
        let description = format!(
            "Type {} for {} is {} {}",
            old_var_type_string,
            key,
            (if not_operator { "never" } else { "always" }),
            &assertion_string
        );

        tast_info.maybe_add_issue(Issue::new(
            IssueKind::RedundantTypeComparison,
            description,
            statements_analyzer.get_hpos(&pos),
        ));
    } else {
        let description = format!(
            "Type {} for {} is {} {}",
            old_var_type_string,
            key,
            (if not_operator { "always" } else { "never" }),
            &assertion_string
        );

        tast_info.maybe_add_issue(Issue::new(
            IssueKind::ImpossibleTypeComparison,
            description,
            statements_analyzer.get_hpos(&pos),
        ));
    }
}
