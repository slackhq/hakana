use serde::{Deserialize, Serialize};
use strum_macros::Display;

use crate::{code_location::HPos, taint::TaintType};

#[derive(Clone, PartialEq, Eq, Hash, Display, Debug, Serialize, Deserialize)]
pub enum IssueKind {
    EmptyBlock,
    FalsableReturnStatement,
    FalseArgument,
    ImpossibleAssignment,
    ImpossibleTypeComparison,
    InternalError,
    InvalidArgument,
    InvalidArrayOffset,
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
    PossiblyNullArrayOffset,
    PossiblyNullIterator,
    PossiblyUndefinedIntArrayOffset,
    PossiblyUndefinedStringArrayOffset,
    PossiblyUnusedProperty,
    PropertyTypeCoercion,
    RedundantTypeComparison,
    TaintedData(TaintType),
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
    UnusedPrivateMethod,
    UnusedProperty,
    UnusedPublicOrProtectedMethod,
    UnusedTrait,
    UnusedVariable,
}

impl IssueKind {
    pub fn from_str(str: &str) -> Result<IssueKind, String> {
        match str {
            "UnusedFunction" => Ok(IssueKind::UnusedFunction),
            "UnusedVariable" => Ok(IssueKind::UnusedVariable),
            "UnusedPrivateMethod" => Ok(IssueKind::UnusedPrivateMethod),
            "UnusedPublicOrProtectedMethod" => Ok(IssueKind::UnusedPublicOrProtectedMethod),
            "InvalidArrayOffset" => Ok(IssueKind::InvalidArrayOffset),
            "EmptyBlock" => Ok(IssueKind::EmptyBlock),
            "InvalidMethodCall" => Ok(IssueKind::InvalidMethodCall),
            "PossiblyInvalidMethodCall" => Ok(IssueKind::PossiblyInvalidMethodCall),
            _ => Err("Unrecognized issue".to_string()),
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
            Self::UnusedVariable => true,
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Issue {
    pub kind: IssueKind,
    pub description: String,
    pub pos: HPos,
    pub functionlike_id: Option<String>,
}

impl Issue {
    pub fn new(kind: IssueKind, description: String, pos: HPos) -> Self {
        Self {
            kind,
            description,
            pos,
            functionlike_id: None,
        }
    }

    pub fn format(&self) -> String {
        format!(
            "ERROR: {} - {}:{}:{} - {}\n",
            self.kind,
            self.pos.file_path,
            self.pos.start_line,
            self.pos.start_column,
            self.description
        )
    }
}
