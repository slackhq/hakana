use hakana_code_info::analysis_result::Replacement;
use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::function_context::FunctionLikeIdentifier;
use hakana_code_info::functionlike_info::FunctionLikeInfo;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::t_atomic::{TAtomic, TGenericParam, TNamedObject};
use hakana_code_info::t_union::TUnion;
use hakana_code_info::type_definition_info::TypeDefinitionInfo;
use hakana_parser::aast;
use hakana_str::{Interner, StrId};

use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;

/// Reports `OverlyBroadReturnType` for a function whose declared return type
/// is a type alias marked `<<Hakana\RequireMostSpecificReturnType>>` (a union
/// of classes such as `ResultWithError = ResultOk<T> | ResultError<TErr>`,
/// named directly or through other aliases such as `Result<T>`), when every
/// value it returns is an instance of the same one of those classes: the
/// return type should then name that class. Type parameters are not
/// compared.
///
/// Left alone: closures; generated code (the generator is what would change);
/// methods that override or implement another, whose return type follows the
/// contract of the method they override; and methods that a subclass
/// overrides or could (trait and abstract methods), whose overrides narrowing
/// the return type would break.
pub(crate) fn check(
    statements_analyzer: &StatementsAnalyzer,
    functionlike_storage: &FunctionLikeInfo,
    context: &BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    return_hint: &aast::TypeHint,
) {
    let (Some(declared_type), Some(return_type_location)) = (
        &functionlike_storage.return_type,
        functionlike_storage.return_type_location,
    ) else {
        return;
    };

    if analysis_data.inferred_return_types.is_empty()
        || analysis_data.inferred_yield_type.is_some()
        || !functionlike_storage.user_defined
        || functionlike_storage.generated
        || functionlike_storage.is_closure
        // A `HAKANA_IGNORE` above the function.
        || functionlike_storage
            .suppressed_issues
            .iter()
            .any(|(kind, _)| *kind == IssueKind::OverlyBroadReturnType)
    {
        return;
    }

    let codebase = statements_analyzer.codebase;
    let is_async = functionlike_storage.is_async;

    let Some((first_alias, required_alias)) = awaited(declared_type, is_async)
        .and_then(|declared_type| get_required_alias(codebase, declared_type))
    else {
        return;
    };

    let Some(variants) = get_variants(required_alias) else {
        return;
    };

    // A `return` that the analysis did not reach (after a call that only
    // returns in tests, say) may return another variant.
    if analysis_data.has_unanalysed_code {
        return;
    }

    match context.function_context.calling_functionlike_id {
        Some(FunctionLikeIdentifier::Function(_)) => {}
        Some(FunctionLikeIdentifier::Method(class_name, method_name)) => {
            // A method of a generated class (`<<Codegen>>` on the class).
            if codebase
                .classlike_infos
                .get(&class_name)
                .is_some_and(|classlike_info| classlike_info.generated)
            {
                return;
            }
            if functionlike_storage.overrides_or_may_be_overridden(
                codebase,
                class_name,
                method_name,
            ) {
                return;
            }
        }
        _ => return,
    }

    let mut returned_variant = None;

    for returned_type in &analysis_data.inferred_return_types {
        let Some(returned_type) = awaited(returned_type, is_async) else {
            return;
        };

        for atomic in &returned_type.types {
            match atomic {
                // Allowed by a nullable return type, and not one of the
                // variants either way.
                TAtomic::TNull | TAtomic::TNothing => {}
                TAtomic::TNamedObject(TNamedObject { name, .. }) => {
                    let Some(variant) = variants.iter().find(|variant| {
                        name == variant.0 || codebase.class_extends_or_implements(name, variant.0)
                    }) else {
                        return;
                    };

                    match returned_variant {
                        None => returned_variant = Some(*variant),
                        Some(existing) if existing.0 == variant.0 => {}
                        Some(_) => return,
                    }
                }
                _ => return,
            }
        }
    }

    let Some((variant_name, variant_type_params)) = returned_variant else {
        return;
    };

    let interner = statements_analyzer.interner;

    // The return type as written (`Result<Foo>`), which says more than the
    // alias it names.
    let declared_type_text = statements_analyzer
        .file_analyzer
        .file_source
        .file_contents
        .get(return_type_location.start_offset as usize..return_type_location.end_offset as usize);

    let variant_display_name = interner.lookup(variant_name);

    let issue = Issue::new(
        IssueKind::OverlyBroadReturnType,
        format!(
            "Every value returned is a {variant_display_name}, so the return type should name {variant_display_name} rather than {}",
            declared_type_text
                .map(str::to_string)
                .unwrap_or_else(|| declared_type.get_id(Some(interner))),
        ),
        return_type_location,
        &context.function_context.calling_functionlike_id,
    );

    // (`should_autofix` fixes test code even where the issue is suppressed:
    // not a return type that is suppressed because tests stub it.)
    if statements_analyzer.should_autofix(context, analysis_data, &issue)
        && analysis_data.get_matching_hakana_fixme(&issue).is_none()
        && let Some(return_hint) = &return_hint.1
        && let Some((offsets, replacement)) = get_replacement(
            statements_analyzer,
            return_hint,
            is_async,
            first_alias,
            required_alias,
            variant_name,
            variant_type_params,
        )
    {
        analysis_data.add_replacement(offsets, Replacement::Substitute(replacement));
    } else {
        analysis_data.maybe_add_issue(
            issue,
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }
}

/// What a function that returns `union` gives the code that calls it, or
/// that awaits it if it is async.
fn awaited(union: &TUnion, is_async: bool) -> Option<&TUnion> {
    if !is_async {
        return Some(union);
    }

    match union.types.as_slice() {
        [TAtomic::TAwaitable { value }] => Some(value),
        _ => None,
    }
}

/// The alias that `declared_type` names (with or without `null`), and the
/// alias marked `<<Hakana\RequireMostSpecificReturnType>>` it leads to, if it
/// leads to one.
fn get_required_alias<'a>(
    codebase: &'a CodebaseInfo,
    declared_type: &TUnion,
) -> Option<(StrId, &'a TypeDefinitionInfo)> {
    let mut non_null = declared_type
        .types
        .iter()
        .filter(|atomic| !matches!(atomic, TAtomic::TNull));

    let (Some(TAtomic::TTypeAlias { name, .. }), None) = (non_null.next(), non_null.next()) else {
        return None;
    };

    let mut alias_name = *name;

    // Aliases of aliases (`Result<T> = ResultWithError<T, mixed>`), within
    // reason.
    for _ in 0..10 {
        let definition = codebase.type_definitions.get(&alias_name)?;

        if requires_most_specific_return_type(definition) {
            return Some((*name, definition));
        }

        match definition.actual_type.types.as_slice() {
            [TAtomic::TTypeAlias { name, .. }] => alias_name = *name,
            _ => return None,
        }
    }

    None
}

/// The classes of the alias, with their type parameters, if it is a union of
/// two classes or more.
fn get_variants(definition: &TypeDefinitionInfo) -> Option<Vec<(&StrId, Option<&Vec<TUnion>>)>> {
    let variants = definition
        .actual_type
        .types
        .iter()
        .map(|variant| match variant {
            TAtomic::TNamedObject(TNamedObject {
                name, type_params, ..
            }) => Some((name, type_params.as_ref())),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;

    if variants.len() > 1 {
        Some(variants)
    } else {
        None
    }
}

fn requires_most_specific_return_type(definition: &TypeDefinitionInfo) -> bool {
    definition
        .attributes
        .iter()
        .any(|attribute| attribute.name == StrId::HAKANA_REQUIRE_MOST_SPECIFIC_RETURN_TYPE)
}

/// The fix: the span, in the file, of the alias that the return type names,
/// and that alias written as the variant (`Awaitable<?Result<Foo>>` →
/// `Awaitable<?ResultOk<Foo>>`). The type arguments of the variant are those
/// of the alias written, carried down the aliases to it
/// (`Result<T> = ResultWithError<T, mixed>`, and
/// `ResultWithError<T, TErr> = ResultOk<T> | ResultError<TErr>`, make
/// `Result<Foo>` either `ResultOk<Foo>` or `ResultError<mixed>`). `None`
/// where the return type is not written as expected, or where a type
/// argument would have to be written from a type other than a type parameter
/// or a simple keyword type.
fn get_replacement(
    statements_analyzer: &StatementsAnalyzer,
    return_hint: &aast::Hint,
    is_async: bool,
    first_alias: StrId,
    required_alias: &TypeDefinitionInfo,
    variant_name: &StrId,
    variant_type_params: Option<&Vec<TUnion>>,
) -> Option<((u32, u32), String)> {
    let codebase = statements_analyzer.codebase;
    let interner = statements_analyzer.interner;
    let file_analyzer = statements_analyzer.file_analyzer;

    let resolved_name = |id: &aast::Id| {
        file_analyzer
            .resolved_names
            .get(&(id.0.start_offset() as u32))
    };
    let text = |pos: &aast::Pos| {
        file_analyzer
            .file_source
            .file_contents
            .get(pos.start_offset()..pos.end_offset())
    };

    // The alias written, inside `Awaitable<...>` and `?`.
    let mut hint = *return_hint;

    if is_async {
        match hint.1 {
            aast::Hint_::Happly(id, [inner]) if resolved_name(id) == Some(&StrId::AWAITABLE) => {
                hint = *inner;
            }
            _ => return None,
        }
    }

    if let aast::Hint_::Hoption(inner) = hint.1 {
        hint = *inner;
    }

    let aast::Hint_::Happly(alias_id, arguments) = hint.1 else {
        return None;
    };

    if resolved_name(alias_id) != Some(&first_alias) {
        return None;
    }

    // The arguments of each alias in turn, as text, down to the one marked.
    let mut alias_name = first_alias;
    let mut arguments = arguments
        .iter()
        .map(|argument| text(&argument.0).map(str::to_string))
        .collect::<Option<Vec<_>>>()?;

    loop {
        let definition = codebase.type_definitions.get(&alias_name)?;

        if arguments.len() != definition.template_types.len() {
            return None;
        }

        if std::ptr::eq(definition, required_alias) {
            break;
        }

        let [
            TAtomic::TTypeAlias {
                name, type_params, ..
            },
        ] = definition.actual_type.types.as_slice()
        else {
            return None;
        };

        arguments = type_params
            .iter()
            .flatten()
            .map(|param| write_type_argument(interner, param, definition, &arguments))
            .collect::<Option<Vec<_>>>()?;
        alias_name = *name;
    }

    let variant_arguments = variant_type_params
        .into_iter()
        .flatten()
        .map(|param| write_type_argument(interner, param, required_alias, &arguments))
        .collect::<Option<Vec<_>>>()?;

    let variant_name = interner.lookup(variant_name);
    let in_namespace = statements_analyzer
        .get_namespace()
        .as_ref()
        .is_some_and(|namespace| !namespace.is_empty());
    let mut replacement = if in_namespace || text(&alias_id.0)?.starts_with('\\') {
        format!("\\{variant_name}")
    } else {
        variant_name.to_string()
    };

    if !variant_arguments.is_empty() {
        replacement += &format!("<{}>", variant_arguments.join(", "));
    }

    Some((
        (hint.0.start_offset() as u32, hint.0.end_offset() as u32),
        replacement,
    ))
}

/// `param`, a type argument in the definition of the alias `definition`
/// whose own arguments are written `arguments`, as text: the argument written
/// for a type parameter of the alias, or a simple keyword type.
fn write_type_argument(
    interner: &Interner,
    param: &TUnion,
    definition: &TypeDefinitionInfo,
    arguments: &[String],
) -> Option<String> {
    match param.types.as_slice() {
        [TAtomic::TGenericParam(TGenericParam { param_name, .. })] => definition
            .template_types
            .iter()
            .position(|(name, _)| name == param_name)
            .map(|index| arguments[index].clone()),
        [TAtomic::TMixed | TAtomic::TMixedWithFlags(..)] => Some("mixed".to_string()),
        [TAtomic::TNothing] => Some("nothing".to_string()),
        [atomic] => {
            let id = atomic.get_id(Some(interner));
            matches!(
                id.as_str(),
                "null" | "int" | "string" | "bool" | "float" | "num" | "arraykey" | "nonnull"
            )
            .then_some(id)
        }
        _ => None,
    }
}
