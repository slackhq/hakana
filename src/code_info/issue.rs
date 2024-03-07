use std::str::FromStr;

use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumString};

use crate::{
    code_location::HPos, function_context::FunctionLikeIdentifier, taint::SinkType, StrId,
};

#[derive(Clone, PartialEq, Eq, Hash, Display, Debug, Serialize, Deserialize, EnumString)]
pub enum IssueKind {
    CannotInferGenericParam,
    CustomIssue(String),
    EmptyBlock,
    FalsableReturnStatement,
    FalseArgument,
    ForLoopInvalidation,
    ImpossibleArrayAssignment,
    ImpossibleAssignment,
    ImpossibleKeyCheck,
    ImpossibleNonnullEntryCheck,
    ImpossibleNullTypeComparison,
    ImpossibleTruthinessCheck,
    ImpossibleTypeComparison,
    IncompatibleTypeParameters,
    InternalError,
    InvalidArgument,
    InvalidArrayOffset,
    InvalidContainsCheck,
    InvalidHackFile,
    InvalidInoutArgument,
    InvalidMethodCall,
    InvalidPropertyAssignmentValue,
    InvalidReturnStatement,
    InvalidReturnType,
    InvalidReturnValue,
    LessSpecificArgument,
    LessSpecificNestedAnyArgumentType,
    LessSpecificNestedAnyReturnStatement,
    LessSpecificNestedArgumentType,
    LessSpecificNestedReturnStatement,
    LessSpecificReturnStatement,
    MethodCallOnNull,
    MissingRequiredXhpAttribute,
    MixedAnyArgument,
    MixedAnyArrayAccess,
    MixedAnyArrayAssignment,
    MixedAnyArrayOffset,
    MixedAnyAssignment,
    MixedAnyMethodCall,
    MixedAnyPropertyAssignment,
    MixedAnyPropertyTypeCoercion,
    MixedAnyReturnStatement,
    MixedArgument,
    MixedArrayAccess,
    MixedArrayAssignment,
    MixedArrayOffset,
    MixedMethodCall,
    MixedOperand,
    MixedPropertyAssignment,
    MixedPropertyTypeCoercion,
    MixedReturnStatement,
    NoValue,
    NonExistentClass,
    NonExistentClassConstant,
    NonExistentClasslike,
    NonExistentConstant,
    NonExistentFile,
    NonExistentFunction,
    NonExistentMethod,
    NonExistentProperty,
    NonExistentType,
    NonExistentXhpAttribute,
    NoJoinInAsyncFunction,
    NonNullableReturnType,
    NothingReturn,
    NullArrayOffset,
    NullIterator,
    NullablePropertyAssignment,
    NullableReturnStatement,
    NullableReturnValue,
    ParadoxicalCondition,
    PossibleMethodCallOnNull,
    PossiblyFalseArgument,
    PossiblyInvalidArgument,
    PossiblyInvalidArrayAccess,
    PossiblyInvalidMethodCall,
    PossiblyNullArrayAccess,
    PossiblyNullArrayOffset,
    PossiblyNullIterator,
    PossiblyNullPropertyFetch,
    PossiblyUndefinedIntArrayOffset,
    PossiblyUndefinedStringArrayOffset,
    PropertyTypeCoercion,
    RedundantIssetCheck,
    RedundantKeyCheck,
    RedundantNonnullEntryCheck,
    RedundantNonnullTypeComparison,
    RedundantTruthinessCheck,
    RedundantTypeComparison,
    TaintedData(SinkType),
    UndefinedIntArrayOffset,
    UndefinedStringArrayOffset,
    UndefinedVariable,
    UnevaluatedCode,
    UnnecessaryShapesIdx,
    UnrecognizedBinaryOp,
    UnrecognizedExpression,
    UnrecognizedStatement,
    UnrecognizedUnaryOp,
    UnusedAssignment,
    UnusedAssignmentInClosure,
    UnusedAssignmentStatement,
    UnusedAwaitable,
    UnusedBuiltinReturnValue,
    UnusedClass,
    UnusedClosureParameter,
    UnusedFunction,
    UnusedFunctionCall,
    UnusedInterface,
    UnusedParameter,
    UnusedPipeVariable,
    UnusedPrivateMethod,
    UnusedPrivateProperty,
    UnusedPublicOrProtectedMethod,
    UnusedPublicOrProtectedProperty,
    UnusedStatement,
    UnusedTrait,
    UnusedTypeDefinition,
    UnusedXhpAttribute,
    UpcastAwaitable,
}

impl IssueKind {
    pub fn from_str_custom(
        str: &str,
        all_custom_issues: &FxHashSet<String>,
    ) -> Result<IssueKind, String> {
        if let Ok(issue_kind) = IssueKind::from_str(str) {
            return Ok(issue_kind);
        }

        if all_custom_issues.contains(str) {
            Ok(IssueKind::CustomIssue(str.to_string()))
        } else {
            Err(format!("Unknown issue {}", str))
        }
    }

    #[allow(clippy::inherent_to_string_shadow_display)]
    pub fn to_string(&self) -> String {
        match self {
            Self::CustomIssue(str) => str.clone(),
            //Self::TaintedData(sink_type) => format!("TaintedData({})", sink_type),
            _ => format!("{}", self),
        }
    }

    pub fn is_mixed_issue(&self) -> bool {
        matches!(
            self,
            Self::LessSpecificNestedAnyArgumentType
                | Self::LessSpecificNestedAnyReturnStatement
                | Self::MixedAnyArgument
                | Self::MixedAnyArrayAccess
                | Self::MixedAnyArrayAssignment
                | Self::MixedAnyArrayOffset
                | Self::MixedAnyAssignment
                | Self::MixedAnyMethodCall
                | Self::MixedAnyPropertyAssignment
                | Self::MixedAnyPropertyTypeCoercion
                | Self::MixedAnyReturnStatement
                | Self::MixedArgument
                | Self::MixedArrayAccess
                | Self::MixedArrayAssignment
                | Self::MixedArrayOffset
                | Self::MixedMethodCall
                | Self::MixedPropertyAssignment
                | Self::MixedPropertyTypeCoercion
                | Self::MixedReturnStatement
        )
    }

    pub fn is_unused_definition(&self) -> bool {
        matches!(
            self,
            Self::UnusedClass
                | Self::UnusedTypeDefinition
                | Self::UnusedFunction
                | Self::UnusedInterface
                | Self::UnusedPrivateProperty
                | Self::UnusedPublicOrProtectedProperty
                | Self::UnusedPublicOrProtectedMethod
                | Self::UnusedXhpAttribute
                | Self::UnusedTrait
        )
    }

    pub fn is_unused_expression(&self) -> bool {
        matches!(
            self,
            Self::UnusedAssignment
                | Self::UnusedAssignmentStatement
                | Self::UnusedAssignmentInClosure
                | Self::UnusedParameter
                | Self::UnusedClosureParameter
                | Self::UnusedPipeVariable
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Issue {
    pub kind: IssueKind,
    pub description: String,
    pub pos: HPos,
    pub can_fix: bool,
    pub fixme_added: bool,
    pub symbol: (StrId, StrId),
}

impl Issue {
    pub fn new(
        kind: IssueKind,
        description: String,
        pos: HPos,
        calling_functionlike_id: &Option<FunctionLikeIdentifier>,
    ) -> Self {
        Self {
            kind,
            description,
            symbol: match calling_functionlike_id {
                Some(FunctionLikeIdentifier::Function(id)) => (*id, StrId::EMPTY),
                Some(FunctionLikeIdentifier::Method(a, b)) => (*a, *b),
                None => (pos.file_path.0, StrId::EMPTY),
                _ => {
                    panic!()
                }
            },
            pos,
            can_fix: false,
            fixme_added: false,
        }
    }

    pub fn format(&self, path: &String) -> String {
        format!(
            "ERROR: {} - {}:{}:{} - {}\n",
            self.kind.to_string(),
            path,
            self.pos.start_line,
            self.pos.start_column,
            self.description
        )
    }
}

pub fn get_issue_from_comment(
    trimmed_text: &str,
    all_custom_issues: &FxHashSet<String>,
) -> Option<Result<IssueKind, String>> {
    if trimmed_text.starts_with("HAKANA_") {
        if let Some(start_bracket_pos) = trimmed_text.find('[') {
            match &trimmed_text[7..start_bracket_pos] {
                "IGNORE" | "FIXME" => {
                    if let Some(end_bracket_pos) = trimmed_text.find(']') {
                        return Some(IssueKind::from_str_custom(
                            &trimmed_text[(start_bracket_pos + 1)..end_bracket_pos],
                            all_custom_issues,
                        ));
                    }
                }
                _ => {}
            }
        }
    } else if trimmed_text == "HHAST_FIXME[UnusedParameter]" {
        return Some(Ok(IssueKind::UnusedParameter));
    } else if trimmed_text == "HHAST_FIXME[UnusedVariable]" {
        return Some(Ok(IssueKind::UnusedAssignment));
    } else if trimmed_text.starts_with("HHAST_FIXME[NoJoinInAsyncFunction]") {
        return Some(Ok(IssueKind::NoJoinInAsyncFunction));
    }

    None
}
