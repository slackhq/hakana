use core::hash::Hash;
use std::{hash::Hasher, str::FromStr};

use hakana_str::StrId;
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumString};

use crate::{
    code_location::{HPos, StmtStart},
    function_context::FunctionLikeIdentifier,
    taint::SinkType,
};

#[derive(Clone, PartialEq, Eq, Hash, Display, Debug, Serialize, Deserialize, EnumString)]
pub enum IssueKind {
    AbstractInstantiation,
    AwaitInSyncContext,
    BannedFunction,
    CannotInferGenericParam,
    CloneInsideLoop,
    CustomIssue(Box<String>),
    DuplicateClassDefinition,
    DuplicateConstantDefinition,
    DuplicateEnumValue,
    DuplicateFunctionDefinition,
    DuplicateTypeDefinition,
    EmptyBlock,
    ExclusiveEnumValueReused,
    ExtendFinalClass,
    FalsableReturnStatement,
    FalseArgument,
    ForLoopInvalidation,
    FunctionCouldBeMadeAsync,
    ImmutablePropertyWrite,
    ImplicitAsioJoin,
    ImplicitStringCast,
    ImpossibleArrayAssignment,
    ImpossibleAssignment,
    ImpossibleKeyCheck,
    ImpossibleNonnullEntryCheck,
    ImpossibleNullTypeComparison,
    ImpossibleTruthinessCheck,
    ImpossibleTypeComparison,
    IncompatibleTypeParameters,
    InternalError,
    InterfaceSingleImplementor,
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
    LargeTypeExpansion,
    LessSpecificArgument,
    LessSpecificNestedAnyArgumentType,
    LessSpecificNestedAnyReturnStatement,
    LessSpecificNestedArgumentType,
    LessSpecificNestedReturnStatement,
    LessSpecificReturnStatement,
    MethodCallOnNull,
    MissingFinalOrAbstract,
    MissingIndirectServiceCallsAttribute,
    MissingInoutToken,
    MissingOverrideAttribute,
    MissingRequiredNamedArgument,
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
    NoJoinInAsyncFunction,
    NonBoolCondition,
    NonExhaustiveSwitchStatement,
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
    NonEnumSwitchValue,
    NonNullableReturnType,
    NothingReturn,
    NoValue,
    NullablePropertyAssignment,
    NullableReturnStatement,
    NullableReturnValue,
    NullArrayOffset,
    NullIterator,
    OnlyUsedInTests,
    ParadoxicalCondition,
    PHPStandardLibrary,
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
    ShadowedLoopVar,
    StrictObjectEquality,
    TaintedData(Box<SinkType>),
    TestOnlyCall,
    TooFewArguments,
    TooManyArguments,
    UndefinedIntArrayOffset,
    UndefinedStringArrayOffset,
    UndefinedVariable,
    UnevaluatedCode,
    UnexpectedNamedArgument,
    UnnecessaryAsyncAnnotation,
    UnnecessaryServiceCallsAttribute,
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
    UnusedInheritedMethod,
    UnusedInoutAssignment,
    UnusedInterface,
    UnusedMethodCall,
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
    UselessControlFlow,
    UselessDefaultCase,
    VarUsedThenRedefinedInLoop,
    AwaitVariableDefinedOutsideIf,
    VariableDefinedOutsideIf,
}

static AUTOFIXABLE_ISSUES: [IssueKind; 24] = [
    IssueKind::UnusedClass,
    IssueKind::UnusedTypeDefinition,
    IssueKind::UnusedFunction,
    IssueKind::UnusedInterface,
    IssueKind::UnusedPrivateProperty,
    IssueKind::UnusedPrivateMethod,
    IssueKind::UnusedInheritedMethod,
    IssueKind::UnusedPublicOrProtectedProperty,
    IssueKind::UnusedPublicOrProtectedMethod,
    IssueKind::UnusedXhpAttribute,
    IssueKind::UnusedTrait,
    IssueKind::OnlyUsedInTests,
    IssueKind::EmptyBlock,
    IssueKind::UnnecessaryShapesIdx,
    IssueKind::UnusedClosureParameter,
    IssueKind::UnusedAssignment,
    IssueKind::UnusedAssignmentStatement,
    IssueKind::ImplicitStringCast,
    IssueKind::NoJoinInAsyncFunction,
    IssueKind::ImplicitAsioJoin,
    IssueKind::ImpossibleNullTypeComparison,
    IssueKind::NonBoolCondition,
    IssueKind::MissingIndirectServiceCallsAttribute,
    IssueKind::RedundantIssetCheck,
];

impl IssueKind {
    pub fn from_str_custom(
        str: &str,
        all_custom_issues: &FxHashSet<String>,
    ) -> Result<IssueKind, String> {
        if let Ok(issue_kind) = IssueKind::from_str(str) {
            return Ok(issue_kind);
        }

        if all_custom_issues.contains(str) {
            Ok(IssueKind::CustomIssue(Box::new(str.to_string())))
        } else {
            Err(format!("Unknown issue {}", str))
        }
    }

    #[allow(clippy::inherent_to_string_shadow_display)]
    pub fn to_string(&self) -> String {
        match self {
            Self::CustomIssue(str) => (**str).clone(),
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
                | Self::UnusedPrivateMethod
                | Self::UnusedInheritedMethod
                | Self::UnusedPublicOrProtectedProperty
                | Self::UnusedPublicOrProtectedMethod
                | Self::UnusedXhpAttribute
                | Self::UnusedTrait
                | Self::OnlyUsedInTests
        )
    }

    pub fn requires_dataflow_analysis(&self) -> bool {
        matches!(
            self,
            Self::UnusedAssignment
                | Self::UnusedAssignmentStatement
                | Self::UnusedInoutAssignment
                | Self::UnusedAssignmentInClosure
                | Self::UnusedParameter
                | Self::UnusedClosureParameter
                | Self::UnusedPipeVariable
                | Self::AwaitVariableDefinedOutsideIf
                | Self::VariableDefinedOutsideIf
        )
    }

    pub fn has_autofix(&self) -> bool {
        AUTOFIXABLE_ISSUES.contains(&self)
    }
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub kind: IssueKind,
    pub description: String,
    pub pos: HPos,
    pub can_fix: bool,
    pub fixme_added: bool,
    pub symbol: (StrId, StrId),
    pub insertion_start: Option<StmtStart>,
}

impl PartialEq for Issue {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.pos == other.pos && self.description == other.description
    }
}

impl Hash for Issue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
        self.pos.hash(state);
        self.description.hash(state);
    }
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
            insertion_start: None,
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
    } else if trimmed_text.starts_with("HHAST_FIXME[FinalOrAbstractClass]") {
        return Some(Ok(IssueKind::MissingFinalOrAbstract));
    } else if trimmed_text.starts_with("HHAST_FIXME[PHPStandardLibrary]") {
        return Some(Ok(IssueKind::PHPStandardLibrary));
    } else if trimmed_text.starts_with("HHAST_FIXME[BannedFunctions]")
        || trimmed_text.starts_with("HHAST_IGNORE_ERROR[BannedFunctions]")
    {
        return Some(Ok(IssueKind::BannedFunction));
    }

    None
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsStr, io, path::PathBuf, str::FromStr};

    use crate::issue::{AUTOFIXABLE_ISSUES, IssueKind};

    #[test]
    fn autofixable_issues_list_should_reflect_reality() -> io::Result<()> {
        let autofix_tests = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent() // src
            .expect("Failed to get parent")
            .parent() // hakana root
            .expect("Failed to get source root")
            .join("tests")
            .join("fix");

        for entry in std::fs::read_dir(&autofix_tests)? {
            let path = entry?.path();
            if path.is_dir() {
                let name = path
                    .file_name()
                    .and_then(&OsStr::to_str)
                    .expect("failed to get file name");
                let issue = IssueKind::from_str(name).expect("failed to get issue");
                assert!(
                    AUTOFIXABLE_ISSUES.contains(&issue),
                    "Issue {} is not marked as autofixable",
                    issue
                );
            }
        }

        for issue in &AUTOFIXABLE_ISSUES {
            assert!(
                autofix_tests.join(issue.to_string()).is_dir(),
                "Issue {} is listed as autofixable but has no tests under tests/fix/ to back up this claim",
                issue
            )
        }

        Ok(())
    }
}
