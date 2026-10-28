use hakana_code_info::codebase_info::CodebaseInfo;
use hakana_code_info::functionlike_identifier::FunctionLikeIdentifier;
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::t_atomic::{TAtomic, TGenericParam, TNamedObject};
use hakana_code_info::t_union::TUnion;
use hakana_str::StrId;
use oxidized::{aast, pos::Pos};

use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::BlockContext;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::statements_analyzer::StatementsAnalyzer;

pub(crate) fn check_legacy_is_truthy(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    argument: &aast::Expr<(), ()>,
    call_pos: &Pos,
) {
    // C\is_empty itself uses legacy_is_truthy for its array-backed cases.
    if context.function_context.calling_functionlike_id
        == Some(FunctionLikeIdentifier::Function(StrId::LIB_C_IS_EMPTY))
    {
        return;
    }

    let Some(argument_type) = analysis_data.get_expr_type(argument.pos()) else {
        return;
    };
    let Some(known_type) = classify_union(argument_type, statements_analyzer.codebase) else {
        return;
    };
    let suggestion = match (known_type.kind, known_type.nullable) {
        (None, false) => return, // No value is produced by an expression of type nothing.
        (None, true) => "this value is always falsy",
        (Some(Kind::Collection), false) => "use !HH\\Lib\\C\\is_empty(...) instead",
        (Some(Kind::Collection), true) => "use a null check and !HH\\Lib\\C\\is_empty(...) instead",
        (Some(Kind::Int), false) => "compare with 0 using !== instead",
        (Some(Kind::Int), true) => "check for nonnull and compare with 0 using !== instead",
        (Some(Kind::Float), false) => "compare with 0.0 using !== instead",
        (Some(Kind::Float), true) => "check for nonnull and compare with 0.0 using !== instead",
        (Some(Kind::Number), false) => "use an explicit numeric zero comparison instead",
        (Some(Kind::Number), true) => {
            "use a null check and an explicit numeric zero comparison instead"
        }
        (Some(Kind::Bool), false) => "use the boolean value directly",
        (Some(Kind::Bool), true) => "compare with true using === instead",
        (Some(Kind::Other), _) => "use an explicit check for this type instead",
    };

    analysis_data.maybe_add_issue(
        Issue::new(
            IssueKind::UnnecessaryLegacyIsTruthy,
            format!(
                "HH\\legacy_is_truthy is unnecessary for type {}; {}",
                argument_type.get_id(Some(statements_analyzer.interner)),
                suggestion,
            ),
            statements_analyzer.get_hpos(call_pos),
            &context.function_context.calling_functionlike_id,
        ),
        statements_analyzer.get_config(),
        statements_analyzer.get_file_path_actual(),
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Collection,
    Int,
    Float,
    Number,
    Bool,
    Other,
}

impl Kind {
    fn is_numeric(self) -> bool {
        matches!(self, Self::Int | Self::Float | Self::Number)
    }
}

#[derive(Default)]
struct KnownType {
    kind: Option<Kind>,
    nullable: bool,
}

impl KnownType {
    fn merge(&mut self, other: Self) {
        self.nullable |= other.nullable;
        self.kind = match (self.kind, other.kind) {
            (None, kind) | (kind, None) => kind,
            (Some(a), Some(b)) if a == b => Some(a),
            (Some(a), Some(b)) if a.is_numeric() && b.is_numeric() => Some(Kind::Number),
            _ => Some(Kind::Other),
        };
    }
}

fn classify_union(type_: &TUnion, codebase: &CodebaseInfo) -> Option<KnownType> {
    let mut known = KnownType::default();
    for atomic in &type_.types {
        // Every possible top-level value must be known and non-string. Strings
        // inside a collection do not affect that collection's truthiness.
        known.merge(classify_atomic(atomic, codebase)?);
    }
    Some(known)
}

fn classify_atomic(atomic: &TAtomic, codebase: &CodebaseInfo) -> Option<KnownType> {
    let kind = match atomic {
        TAtomic::TNull => {
            return Some(KnownType {
                kind: None,
                nullable: true,
            });
        }
        TAtomic::TNothing => return Some(KnownType::default()),
        TAtomic::TInt | TAtomic::TLiteralInt { .. } => Kind::Int,
        TAtomic::TFloat => Kind::Float,
        TAtomic::TNum => Kind::Number,
        TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse => Kind::Bool,
        TAtomic::TVec(_) | TAtomic::TDict(_) | TAtomic::TKeyset { .. } => Kind::Collection,
        TAtomic::TGenericParam(TGenericParam { as_type, .. })
        | TAtomic::TClassTypeConstant { as_type, .. }
        | TAtomic::TTypeAlias {
            as_type: Some(as_type),
            ..
        } => return classify_union(as_type, codebase),
        TAtomic::TEnum {
            underlying_type,
            as_type,
            ..
        }
        | TAtomic::TEnumLiteralCase {
            underlying_type,
            as_type,
            ..
        } => {
            return classify_atomic(underlying_type.as_ref().or(as_type.as_ref())?, codebase);
        }
        // These interfaces also admit primitive strings.
        TAtomic::TNamedObject(TNamedObject {
            name: StrId::STRINGISH | StrId::XHP_CHILD,
            ..
        }) => return None,
        TAtomic::TNamedObject(TNamedObject { name, .. }) => {
            if matches!(name, &StrId::CONTAINER | &StrId::ANY_ARRAY)
                || codebase.class_extends_or_implements(name, &StrId::CONTAINER)
            {
                Kind::Collection
            } else {
                Kind::Other
            }
        }
        TAtomic::TObjectIntersection { types } => {
            let mut known = KnownType::default();
            for atomic in types {
                known.merge(classify_atomic(atomic, codebase)?);
            }
            return Some(known);
        }
        TAtomic::TObject
        | TAtomic::TAwaitable { .. }
        | TAtomic::TClosure(_)
        | TAtomic::TClosureAlias { .. }
        | TAtomic::TEnumClassLabel { .. }
        | TAtomic::TResource => Kind::Other,
        // Preserve string-like types, arraykey/scalar, mixed (including any and
        // refined mixed), and unresolved or opaque types whose values may be strings.
        _ => return None,
    };
    Some(KnownType {
        kind: Some(kind),
        nullable: false,
    })
}
