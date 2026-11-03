use crate::{
    expr::expression_identifier::get_static_functionlike_id_from_call,
    function_analysis_data::FunctionAnalysisData,
    scope::{BlockContext, response_context::ResponseContentType},
    statements_analyzer::StatementsAnalyzer,
};
use hakana_code_info::{
    EFFECT_WRITE_GLOBALS,
    function_context::FunctionLikeIdentifier,
    functionlike_info::{FunctionLikeInfo, ResponseEffect},
    functionlike_parameter::DefaultType,
    method_identifier::MethodIdentifier,
    t_atomic::TAtomic,
    t_union::TUnion,
};
use hakana_str::StrId;
use oxidized::{aast, pos::Pos};

pub(crate) fn apply(
    analyzer: &StatementsAnalyzer,
    call: &aast::CallExpr<(), ()>,
    pos: &Pos,
    data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let id = get_static_functionlike_id_from_call(
        call,
        analyzer.interner,
        analyzer.file_analyzer.resolved_names,
    )
    .or_else(|| {
        // Only a single non-null receiver can guarantee a response effect.
        if let aast::Expr_::ObjGet(access) = &call.func.2
            && let Some(receiver) = data.get_expr_type(access.0.pos())
            && receiver.types.len() == 1
        {
            crate::expr::expression_identifier::get_method_id_from_call(
                call,
                analyzer.interner,
                &data.expr_types,
            )
        } else {
            None
        }
    });
    let wrapper = id.as_ref().and_then(|id| wrapper_info(analyzer, id));
    let wrapper_effect = wrapper.and_then(FunctionLikeInfo::get_response_effect);
    if wrapper_effect.is_some() && call.unpacked_arg.is_some() {
        context.response.forget();
        data.response_output_events += 1;
        return;
    }
    if wrapper_effect == Some(ResponseEffect::Output) {
        let output = call
            .args
            .first()
            .and_then(|arg| data.get_expr_type(arg.to_expr_ref().pos()));
        if output.is_none_or(|ty| !is_scalar_output(ty)) {
            context.response.invalidate();
        }
        if output.is_none_or(|ty| !is_empty_output(ty)) {
            context.response.emit();
            data.response_output_events += 1;
        }
        return;
    }
    let mut name = match &id {
        Some(FunctionLikeIdentifier::Function(name)) => Some(*name),
        _ => None,
    };
    if wrapper_effect == Some(ResponseEffect::Header) {
        // Wrappers can have additional Location handling (cookies, redirects).
        // Only literal non-Location calls make the audited header() guarantee.
        if !call.args.first()
            .and_then(|arg| data.get_expr_type(arg.to_expr_ref().pos()))
            .is_some_and(|ty| !ty.types.is_empty() && ty.types.iter().all(|t| {
                matches!(t, TAtomic::TLiteralString { value }
                    if value.split_once(':').is_some_and(|(name, _)| !name.trim().eq_ignore_ascii_case("location"))
                        || value.starts_with("HTTP/"))
            }))
        {
            context.response.invalidate();
            return;
        }
        name = Some(StrId::HEADER);
    }
    match name {
        Some(StrId::ECHO | StrId::PRINT) => return, // Handled per argument, in evaluation order.
        Some(StrId::HEADER) => {
            let values = call
                .args
                .first()
                .and_then(|arg| data.get_expr_type(arg.to_expr_ref().pos()));
            let replaces = call.args.get(1).map_or_else(
                || {
                    wrapper.is_none_or(|info| {
                        info.params.get(1).is_none_or(|param| {
                            matches!(
                                param.default_type.as_deref(),
                                Some(DefaultType::NormalData(TAtomic::TTrue))
                            )
                        })
                    })
                },
                |arg| {
                    data.get_expr_type(arg.to_expr_ref().pos())
                        .is_some_and(|ty| {
                            !ty.types.is_empty()
                                && ty.types.iter().all(|t| matches!(t, TAtomic::TTrue))
                        })
                },
            );
            let before = context.response;
            let mut after = None;
            if let Some(values) = values {
                for value in &values.types {
                    let mut branch = before;
                    if let TAtomic::TLiteralString { value } = value {
                        if let Some((key, mime)) = value.split_once(':') {
                            if key.trim().eq_ignore_ascii_case("Content-Type") {
                                let mime = mime.split(';').next().unwrap_or("").trim();
                                let content_type = if replaces
                                    && (mime.eq_ignore_ascii_case("application/json")
                                        || mime.eq_ignore_ascii_case("text/plain"))
                                {
                                    ResponseContentType::NonHtml
                                } else if replaces && mime.eq_ignore_ascii_case("text/html") {
                                    ResponseContentType::Html
                                } else {
                                    ResponseContentType::Unknown
                                };
                                branch.set_content_type(content_type);
                            }
                        } else if !value.starts_with("HTTP/") {
                            branch.invalidate();
                        }
                    } else {
                        branch.invalidate();
                    }
                    after = Some(after.map_or(
                        branch,
                        |previous: crate::scope::response_context::ResponseContext| {
                            previous.join(branch)
                        },
                    ));
                }
            }
            context.response = after.unwrap_or_else(|| {
                let mut unknown = before;
                unknown.invalidate();
                unknown
            });
            return;
        }
        Some(StrId::HEADER_REMOVE) => {
            let key = call
                .args
                .first()
                .and_then(|arg| data.get_expr_type(arg.to_expr_ref().pos()))
                .and_then(|ty| ty.get_single_literal_string_value());
            if key
                .as_ref()
                .is_none_or(|key| key.eq_ignore_ascii_case("Content-Type"))
            {
                context
                    .response
                    .set_content_type(ResponseContentType::Unknown);
            }
            return;
        }
        Some(StrId::VAR_DUMP | StrId::PRINTF) => {
            if call.args.iter().any(|arg| {
                data.get_expr_type(arg.to_expr_ref().pos())
                    .is_none_or(|ty| !is_scalar_output(ty))
            }) {
                context.response.invalidate();
            }
            context.response.emit();
            data.response_output_events += 1;
            return;
        }
        Some(
            StrId::OB_START
            | StrId::OB_CLEAN
            | StrId::OB_FLUSH
            | StrId::OB_END_CLEAN
            | StrId::OB_END_FLUSH
            | StrId::OB_GET_CLEAN
            | StrId::OB_GET_FLUSH
            | StrId::OB_GZHANDLER
            | StrId::OB_ICONV_HANDLER
            | StrId::OB_IMPLICIT_FLUSH
            | StrId::FLUSH
            | StrId::HEADER_REGISTER_CALLBACK,
        ) => {
            context.response.forget();
            data.response_output_events += 1;
            return;
        }
        Some(
            StrId::HEADERS_LIST
            | StrId::HEADERS_SENT
            | StrId::HEADERS_SENT_WITH_FILE_LINE
            | StrId::HTTP_RESPONSE_CODE
            | StrId::SETCOOKIE
            | StrId::SETRAWCOOKIE
            | StrId::OB_GET_CONTENTS
            | StrId::OB_GET_LENGTH
            | StrId::OB_GET_LEVEL
            | StrId::OB_GET_STATUS
            | StrId::OB_LIST_HANDLERS,
        ) => return,
        Some(StrId::JSON_ENCODE | StrId::JSON_ENCODE_WITH_ERROR) => {
            // Assume serialization callbacks preserve HTTP response state.
            // Argument expressions have already applied their own response effects.
            return;
        }
        _ => {}
    }
    // Coeffects that cannot write globals cannot alter HTTP headers or emit.
    // Include argument effects: evaluating a pure call's argument may be impure.
    if data
        .expr_effects
        .get(&(pos.start_offset() as u32, pos.end_offset() as u32))
        .is_some_and(|effects| effects & EFFECT_WRITE_GLOBALS == 0)
    {
        return;
    }
    context.response.invalidate();
}

pub(crate) fn wrapper_info<'a>(
    analyzer: &'a StatementsAnalyzer,
    id: &FunctionLikeIdentifier,
) -> Option<&'a FunctionLikeInfo> {
    let info = match id {
        FunctionLikeIdentifier::Method(class, method)
            if analyzer.codebase.method_exists(class, method) =>
        {
            let declaring = analyzer
                .codebase
                .get_declaring_method_id(&MethodIdentifier(*class, *method));
            analyzer.codebase.get_method(&declaring)
        }
        _ => analyzer.codebase.get_functionlike(id),
    }?;
    info.get_response_effect().map(|_| info)
}

pub(crate) fn is_empty_output(ty: &TUnion) -> bool {
    !ty.types.is_empty()
        && ty.types.iter().all(|t| {
            matches!(t, TAtomic::TFalse | TAtomic::TNull)
                || matches!(t, TAtomic::TLiteralString { value } if value.is_empty())
        })
}

/// Stringification of an object can run arbitrary __toString code.
pub(crate) fn is_scalar_output(ty: &TUnion) -> bool {
    !ty.types.is_empty()
        && ty.types.iter().all(|t| {
            matches!(
                t,
                TAtomic::TString
                    | TAtomic::TStringWithFlags(..)
                    | TAtomic::TLiteralString { .. }
                    | TAtomic::TFalse
                    | TAtomic::TTrue
                    | TAtomic::TNull
                    | TAtomic::TBool
                    | TAtomic::TInt
                    | TAtomic::TLiteralInt { .. }
                    | TAtomic::TFloat
                    | TAtomic::TNum
                    | TAtomic::TArraykey { .. }
                    | TAtomic::TScalar
            )
        })
}
