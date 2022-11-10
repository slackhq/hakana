use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use strum_macros::Display;

use crate::{code_location::HPos, taint::SinkType};

#[derive(Clone, PartialEq, Eq, Hash, Display, Debug, Serialize, Deserialize)]
pub enum IssueKind {
    CannotInferGenericParam,
    CustomIssue(String),
    EmptyBlock,
    FalsableReturnStatement,
    FalseArgument,
    ImpossibleAssignment,
    ImpossibleNullTypeComparison,
    ImpossibleTypeComparison,
    InternalError,
    InvalidArgument,
    InvalidArrayOffset,
    InvalidContainsCheck,
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
    MixedPropertyAssignment,
    MixedPropertyTypeCoercion,
    MixedReturnStatement,
    NoValue,
    NonExistentProperty,
    NonExistentClass,
    NonExistentFunction,
    NonExistentMethod,
    NonNullableReturnType,
    NothingReturn,
    NullArgument,
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
    PossiblyNullArgument,
    PossiblyNullArrayAccess,
    PossiblyNullArrayOffset,
    PossiblyNullIterator,
    PossiblyNullPropertyFetch,
    PossiblyUndefinedIntArrayOffset,
    PossiblyUndefinedStringArrayOffset,
    PossiblyUnusedProperty,
    UnnecessaryShapesIdx,
    PropertyTypeCoercion,
    RedundantNonnullTypeComparison,
    RedundantTypeComparison,
    TaintedData(SinkType),
    UndefinedIntArrayOffset,
    UndefinedStringArrayOffset,
    UndefinedVariable,
    UnevaluatedCode,
    UnrecognizedBinaryOp,
    UnrecognizedExpression,
    UnrecognizedStatement,
    UnrecognizedUnaryOp,
    UnusedClass,
    UnusedFunction,
    UnusedInterface,
    UnusedParameter,
    UnusedPipeVariable,
    UnusedPrivateMethod,
    UnusedProperty,
    UnusedPublicOrProtectedMethod,
    UnusedTrait,
    UnusedAssignment,
    UnusedAssignmentInClosure,
}

impl IssueKind {
    pub fn from_str(str: &str, all_custom_issues: &FxHashSet<String>) -> Result<IssueKind, String> {
        match str {
            "CannotInferGenericParam" => Ok(IssueKind::CannotInferGenericParam),
            "EmptyBlock" => Ok(IssueKind::EmptyBlock),
            "FalsableReturnStatement" => Ok(IssueKind::FalsableReturnStatement),
            "FalseArgument" => Ok(IssueKind::FalseArgument),
            "ImpossibleAssignment" => Ok(IssueKind::ImpossibleAssignment),
            "ImpossibleNullTypeComparison" => Ok(IssueKind::ImpossibleNullTypeComparison),
            "ImpossibleTypeComparison" => Ok(IssueKind::ImpossibleTypeComparison),
            "InternalError" => Ok(IssueKind::InternalError),
            "InvalidArgument" => Ok(IssueKind::InvalidArgument),
            "InvalidArrayOffset" => Ok(IssueKind::InvalidArrayOffset),
            "InvalidContainsCheck" => Ok(IssueKind::InvalidContainsCheck),
            "InvalidMethodCall" => Ok(IssueKind::InvalidMethodCall),
            "InvalidPropertyAssignmentValue" => Ok(IssueKind::InvalidPropertyAssignmentValue),
            "InvalidReturnStatement" => Ok(IssueKind::InvalidReturnStatement),
            "InvalidReturnType" => Ok(IssueKind::InvalidReturnType),
            "InvalidReturnValue" => Ok(IssueKind::InvalidReturnValue),
            "LessSpecificArgument" => Ok(IssueKind::LessSpecificArgument),
            "LessSpecificNestedAnyArgumentType" => Ok(IssueKind::LessSpecificNestedAnyArgumentType),
            "LessSpecificNestedAnyReturnStatement" => {
                Ok(IssueKind::LessSpecificNestedAnyReturnStatement)
            }
            "LessSpecificNestedArgumentType" => Ok(IssueKind::LessSpecificNestedArgumentType),
            "LessSpecificNestedReturnStatement" => Ok(IssueKind::LessSpecificNestedReturnStatement),
            "LessSpecificReturnStatement" => Ok(IssueKind::LessSpecificReturnStatement),
            "MethodCallOnNull" => Ok(IssueKind::MethodCallOnNull),
            "MixedAnyArgument" => Ok(IssueKind::MixedAnyArgument),
            "MixedAnyArrayAccess" => Ok(IssueKind::MixedAnyArrayAccess),
            "MixedAnyArrayAssignment" => Ok(IssueKind::MixedAnyArrayAssignment),
            "MixedAnyArrayOffset" => Ok(IssueKind::MixedAnyArrayOffset),
            "MixedAnyAssignment" => Ok(IssueKind::MixedAnyAssignment),
            "MixedAnyMethodCall" => Ok(IssueKind::MixedAnyMethodCall),
            "MixedAnyPropertyAssignment" => Ok(IssueKind::MixedAnyPropertyAssignment),
            "MixedAnyPropertyTypeCoercion" => Ok(IssueKind::MixedAnyPropertyTypeCoercion),
            "MixedAnyReturnStatement" => Ok(IssueKind::MixedAnyReturnStatement),
            "MixedArgument" => Ok(IssueKind::MixedArgument),
            "MixedArrayAccess" => Ok(IssueKind::MixedArrayAccess),
            "MixedArrayAssignment" => Ok(IssueKind::MixedArrayAssignment),
            "MixedArrayOffset" => Ok(IssueKind::MixedArrayOffset),
            "MixedMethodCall" => Ok(IssueKind::MixedMethodCall),
            "MixedPropertyAssignment" => Ok(IssueKind::MixedPropertyAssignment),
            "MixedPropertyTypeCoercion" => Ok(IssueKind::MixedPropertyTypeCoercion),
            "MixedReturnStatement" => Ok(IssueKind::MixedReturnStatement),
            "NoValue" => Ok(IssueKind::NoValue),
            "NonExistentProperty" => Ok(IssueKind::NonExistentProperty),
            "NonExistentClass" => Ok(IssueKind::NonExistentClass),
            "NonExistentFunction" => Ok(IssueKind::NonExistentFunction),
            "NonExistentMethod" => Ok(IssueKind::NonExistentMethod),
            "NonNullableReturnType" => Ok(IssueKind::NonNullableReturnType),
            "NothingReturn" => Ok(IssueKind::NothingReturn),
            "NullArgument" => Ok(IssueKind::NullArgument),
            "NullArrayOffset" => Ok(IssueKind::NullArrayOffset),
            "NullIterator" => Ok(IssueKind::NullIterator),
            "NullablePropertyAssignment" => Ok(IssueKind::NullablePropertyAssignment),
            "NullableReturnStatement" => Ok(IssueKind::NullableReturnStatement),
            "NullableReturnValue" => Ok(IssueKind::NullableReturnValue),
            "ParadoxicalCondition" => Ok(IssueKind::ParadoxicalCondition),
            "PossibleMethodCallOnNull" => Ok(IssueKind::PossibleMethodCallOnNull),
            "PossiblyFalseArgument" => Ok(IssueKind::PossiblyFalseArgument),
            "PossiblyInvalidArgument" => Ok(IssueKind::PossiblyInvalidArgument),
            "PossiblyInvalidArrayAccess" => Ok(IssueKind::PossiblyInvalidArrayAccess),
            "PossiblyInvalidMethodCall" => Ok(IssueKind::PossiblyInvalidMethodCall),
            "PossiblyNullArgument" => Ok(IssueKind::PossiblyNullArgument),
            "PossiblyNullArrayAccess" => Ok(IssueKind::PossiblyNullArrayAccess),
            "PossiblyNullArrayOffset" => Ok(IssueKind::PossiblyNullArrayOffset),
            "PossiblyNullIterator" => Ok(IssueKind::PossiblyNullIterator),
            "PossiblyUndefinedIntArrayOffset" => Ok(IssueKind::PossiblyUndefinedIntArrayOffset),
            "PossiblyUndefinedStringArrayOffset" => {
                Ok(IssueKind::PossiblyUndefinedStringArrayOffset)
            }
            "PossiblyUnusedProperty" => Ok(IssueKind::PossiblyUnusedProperty),
            "PropertyTypeCoercion" => Ok(IssueKind::PropertyTypeCoercion),
            "RedundantNonnullTypeComparison" => Ok(IssueKind::RedundantNonnullTypeComparison),
            "RedundantTypeComparison" => Ok(IssueKind::RedundantTypeComparison),
            "UndefinedIntArrayOffset" => Ok(IssueKind::UndefinedIntArrayOffset),
            "UndefinedStringArrayOffset" => Ok(IssueKind::UndefinedStringArrayOffset),
            "UndefinedVariable" => Ok(IssueKind::UndefinedVariable),
            "UnevaluatedCode" => Ok(IssueKind::UnevaluatedCode),
            "UnnecessaryShapesIdx" => Ok(IssueKind::UnnecessaryShapesIdx),
            "UnrecognizedBinaryOp" => Ok(IssueKind::UnrecognizedBinaryOp),
            "UnrecognizedExpression" => Ok(IssueKind::UnrecognizedExpression),
            "UnrecognizedStatement" => Ok(IssueKind::UnrecognizedStatement),
            "UnrecognizedUnaryOp" => Ok(IssueKind::UnrecognizedUnaryOp),
            "UnusedClass" => Ok(IssueKind::UnusedClass),
            "UnusedFunction" => Ok(IssueKind::UnusedFunction),
            "UnusedInterface" => Ok(IssueKind::UnusedInterface),
            "UnusedParameter" => Ok(IssueKind::UnusedParameter),
            "UnusedPipeVariable" => Ok(IssueKind::UnusedPipeVariable),
            "UnusedPrivateMethod" => Ok(IssueKind::UnusedPrivateMethod),
            "UnusedProperty" => Ok(IssueKind::UnusedProperty),
            "UnusedPublicOrProtectedMethod" => Ok(IssueKind::UnusedPublicOrProtectedMethod),
            "UnusedTrait" => Ok(IssueKind::UnusedTrait),
            "UnusedAssignment" => Ok(IssueKind::UnusedAssignment),
            "UnusedAssignmentInClosure" => Ok(IssueKind::UnusedAssignmentInClosure),
            _ => {
                if all_custom_issues.contains(str) {
                    Ok(IssueKind::CustomIssue(str.to_string()))
                } else {
                    Err(format!("Unknown issue {}", str))
                }
            }
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            Self::CustomIssue(str) => str.clone(),
            //Self::TaintedData(sink_type) => format!("TaintedData({})", sink_type),
            _ => format!("{}", self),
        }
    }

    pub fn is_mixed_issue(&self) -> bool {
        match &self {
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
            | Self::MixedReturnStatement => true,
            _ => false,
        }
    }

    pub fn is_unused_definition(&self) -> bool {
        match &self {
            Self::UnusedClass
            | Self::UnusedFunction
            | Self::UnusedInterface
            | Self::UnusedProperty
            | Self::UnusedPublicOrProtectedMethod
            | Self::UnusedTrait => true,
            _ => false,
        }
    }

    pub fn is_unused_expression(&self) -> bool {
        match &self {
            Self::UnusedAssignment => true,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Issue {
    pub kind: IssueKind,
    pub description: String,
    pub pos: HPos,
    pub functionlike_id: Option<String>,
    pub can_fix: bool,
}

impl Issue {
    pub fn new(kind: IssueKind, description: String, pos: HPos) -> Self {
        Self {
            kind,
            description,
            pos,
            functionlike_id: None,
            can_fix: false,
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
) -> Option<IssueKind> {
    if trimmed_text.starts_with("HAKANA_") {
        if let Some(start_bracket_pos) = trimmed_text.find("[") {
            match &trimmed_text[7..start_bracket_pos] {
                "IGNORE" | "FIXME" => {
                    if let Some(end_bracket_pos) = trimmed_text.find("]") {
                        return Some(
                            IssueKind::from_str(
                                &trimmed_text[(start_bracket_pos + 1)..end_bracket_pos],
                                all_custom_issues,
                            )
                            .unwrap(),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    return None;
}
