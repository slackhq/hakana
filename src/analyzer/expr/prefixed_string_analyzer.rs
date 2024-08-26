use hakana_code_info::t_atomic::{TAtomic, TDict};
use hakana_str::StrId;

use hakana_code_info::t_union::TUnion;
use hakana_code_info::ttype::wrap_atomic;

use std::sync::Arc;

use hakana_code_info::t_atomic::DictKey;

use std::collections::BTreeMap;

use std::rc::Rc;

use hakana_code_info::ttype::get_string;

use crate::expression_analyzer;

use crate::scope::BlockContext;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::stmt_analyzer::AnalysisError;

use oxidized::aast;

use crate::statements_analyzer::StatementsAnalyzer;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    boxed: &(String, aast::Expr<(), ()>),
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    expr: &aast::Expr<(), ()>,
) -> Result<(), AnalysisError> {
    expression_analyzer::analyze(statements_analyzer, &boxed.1, analysis_data, context)?;

    let inner_type = if let Some(t) = analysis_data.expr_types.get(&(
        boxed.1.pos().start_offset() as u32,
        boxed.1.pos().end_offset() as u32,
    )) {
        (**t).clone()
    } else {
        get_string()
    };

    analysis_data.expr_types.insert(
        (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
        Rc::new(if boxed.0 == "re" {
            let mut inner_text = inner_type.get_single_literal_string_value().unwrap();
            let first_char = inner_text[0..1].to_string();
            let shape_fields;
            if let Some(last_pos) = inner_text.rfind(&first_char) {
                if last_pos > 1 {
                    inner_text = inner_text[1..last_pos].to_string();
                }

                shape_fields = get_shape_fields_from_regex(&inner_text);
            } else {
                shape_fields = BTreeMap::new();
            }

            wrap_atomic(TAtomic::TTypeAlias {
                name: StrId::LIB_REGEX_PATTERN,
                type_params: Some(vec![wrap_atomic(TAtomic::TDict(TDict {
                    known_items: if !shape_fields.is_empty() {
                        Some(shape_fields)
                    } else {
                        None
                    },
                    params: None,
                    non_empty: true,
                    shape_name: None,
                }))]),
                as_type: Some(Box::new(wrap_atomic(TAtomic::TLiteralString {
                    value: inner_text,
                }))),
            })
        } else {
            inner_type
        }),
    );

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn get_shape_fields_from_regex(inner_text: &str) -> BTreeMap<DictKey, (bool, Arc<TUnion>)> {
    let regex = pcre2::bytes::RegexBuilder::new()
        .utf(true)
        .build(inner_text);

    let mut shape_fields = BTreeMap::new();

    if let Ok(regex) = regex {
        for (i, v) in regex.capture_names().iter().enumerate() {
            if let Some(v) = v {
                shape_fields.insert(DictKey::String(v.clone()), (false, Arc::new(get_string())));
            } else {
                shape_fields.insert(DictKey::Int(i as u64), (false, Arc::new(get_string())));
            }
        }
    }

    shape_fields
}

#[cfg(target_arch = "wasm32")]
fn get_shape_fields_from_regex(inner_text: &String) -> BTreeMap<DictKey, (bool, Arc<TUnion>)> {
    let inner_text = inner_text.replace("(?<", "(?P<");
    let regex = regex::Regex::new(&inner_text);

    let mut shape_fields = BTreeMap::new();

    if let Ok(regex) = regex {
        let mut i = 0;
        for v in regex.capture_names() {
            if let Some(v) = v {
                shape_fields.insert(
                    DictKey::String(v.to_string()),
                    (false, Arc::new(get_string())),
                );
            } else {
                shape_fields.insert(DictKey::Int(i as u32), (false, Arc::new(get_string())));
            }
            i += 1;
        }
    }

    shape_fields
}
