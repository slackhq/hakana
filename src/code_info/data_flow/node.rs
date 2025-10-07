use std::hash::{Hash, Hasher};

use crate::VarId;
use crate::code_location::FilePath;
use crate::function_context::FunctionLikeIdentifier;
use crate::method_identifier::MethodIdentifier;
use crate::{
    code_location::HPos,
    taint::{SinkType, SourceType},
};
use hakana_str::{Interner, StrId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VariableSourceKind {
    Default,
    PrivateParam,
    NonPrivateParam,
    InoutParam,
    InoutArg,
    ClosureParam,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
pub enum DataFlowNodeId {
    String(String),
    LocalString(String, FilePath, u32, u32),
    ArrayAssignment(FilePath, u32, u32),
    ArrayItem(String, FilePath, u32, u32),
    Return(FilePath, u32, u32),
    ForInit(u32, u32),
    Composition(FilePath, u32, u32),
    Var(VarId, FilePath, u32, u32),
    VarNarrowedTo(String, StrId, FilePath, u32),
    Param(VarId, FilePath, u32, u32),
    UnlabelledSink(FilePath, u32, u32),
    ReferenceTo(FunctionLikeIdentifier),
    CallTo(FunctionLikeIdentifier),
    SpecializedCallTo(FunctionLikeIdentifier, FilePath, u32),
    FunctionLikeArg(FunctionLikeIdentifier, u8),
    SpecializedFunctionLikeArg(FunctionLikeIdentifier, u8, FilePath, u32),
    Property(StrId, StrId),
    SpecializedProperty(StrId, StrId, FilePath, u32, u32),
    PropertyFetch(VarId, StrId, FilePath, u32),
    FunctionLikeOut(FunctionLikeIdentifier, u8),
    SpecializedFunctionLikeOut(FunctionLikeIdentifier, u8, FilePath, u32),
    ThisBeforeMethod(MethodIdentifier),
    SpecializedThisBeforeMethod(MethodIdentifier, FilePath, u32),
    ThisAfterMethod(MethodIdentifier),
    SpecializedThisAfterMethod(MethodIdentifier, FilePath, u32),
    Symbol(StrId),
    ShapeFieldAccess(StrId, String),
    InstanceMethodCall(FilePath, u32, u32),
}

impl DataFlowNodeId {
    pub fn to_string(&self, interner: &Interner) -> String {
        match self {
            DataFlowNodeId::String(str) => str.clone(),
            DataFlowNodeId::LocalString(str, file_path, start_offset, end_offset) => {
                format!("{}-{}:{}-{}", str, file_path.0.0, start_offset, end_offset)
            }
            DataFlowNodeId::Param(var_id, file_path, start_offset, end_offset) => {
                format!(
                    "param-{}-{}:{}-{}",
                    interner.lookup(&var_id.0),
                    file_path.0.0,
                    start_offset,
                    end_offset
                )
            }
            DataFlowNodeId::Var(var_id, file_path, start_offset, end_offset) => {
                format!(
                    "{}-{}:{}-{}",
                    interner.lookup(&var_id.0),
                    file_path.0.0,
                    start_offset,
                    end_offset
                )
            }
            DataFlowNodeId::VarNarrowedTo(var_id, symbol, file_path, start_offset) => {
                format!(
                    "{} narrowed to {}-{}:{}",
                    var_id,
                    interner.lookup(symbol),
                    file_path.0.0,
                    start_offset
                )
            }
            DataFlowNodeId::ArrayAssignment(file_path, start_offset, end_offset) => {
                format!(
                    "array-assignment-{}:{}-{}",
                    file_path.0.0, start_offset, end_offset
                )
            }
            DataFlowNodeId::ArrayItem(key_value, file_path, start_offset, end_offset) => {
                format!(
                    "array[{}]-{}:{}-{}",
                    key_value, file_path.0.0, start_offset, end_offset
                )
            }
            DataFlowNodeId::Return(file_path, start_offset, end_offset) => {
                format!("return-{}:{}-{}", file_path.0.0, start_offset, end_offset)
            }
            DataFlowNodeId::CallTo(functionlike_id) => {
                format!("call to {}", functionlike_id.to_string(interner))
            }
            DataFlowNodeId::SpecializedCallTo(functionlike_id, file_path, start_offset) => {
                format!(
                    "call to {}-{}:{}",
                    functionlike_id.to_string(interner),
                    file_path.0.0,
                    start_offset
                )
            }
            DataFlowNodeId::Property(classlike_name, property_name) => format!(
                "{}::${}",
                interner.lookup(classlike_name),
                interner.lookup(property_name)
            ),
            DataFlowNodeId::SpecializedProperty(
                classlike_name,
                property_name,
                file_path,
                start_offset,
                end_offset,
            ) => format!(
                "{}::${}-{}:{}-{}",
                interner.lookup(classlike_name),
                interner.lookup(property_name),
                file_path.0.0,
                start_offset,
                end_offset
            ),
            DataFlowNodeId::FunctionLikeOut(functionlike_id, arg) => {
                format!("out {}#{}", functionlike_id.to_string(interner), (arg + 1))
            }
            DataFlowNodeId::SpecializedFunctionLikeOut(
                functionlike_id,
                arg,
                file_path,
                start_offset,
            ) => {
                format!(
                    "out {}#{}-{}:{}",
                    functionlike_id.to_string(interner),
                    (arg + 1),
                    file_path.0.0,
                    start_offset
                )
            }
            DataFlowNodeId::FunctionLikeArg(functionlike_id, arg) => {
                format!("{}#{}", functionlike_id.to_string(interner), (arg + 1))
            }
            DataFlowNodeId::SpecializedFunctionLikeArg(
                functionlike_id,
                arg,
                file_path,
                start_offset,
            ) => {
                format!(
                    "{}#{}-{}:{}",
                    functionlike_id.to_string(interner),
                    (arg + 1),
                    file_path.0.0,
                    start_offset
                )
            }
            DataFlowNodeId::PropertyFetch(lhs_var_id, property_name, file_path, start_offset) => {
                format!(
                    "{}->{}-{}:{}",
                    interner.lookup(&lhs_var_id.0),
                    interner.lookup(property_name),
                    file_path.0.0,
                    start_offset,
                )
            }
            DataFlowNodeId::ThisBeforeMethod(method_id) => format!(
                "$this in {} before {}",
                interner.lookup(&method_id.0),
                interner.lookup(&method_id.1)
            ),
            DataFlowNodeId::SpecializedThisBeforeMethod(method_id, file_path, start_offset) => {
                format!(
                    "$this in {} before {}-{}:{}",
                    interner.lookup(&method_id.0),
                    interner.lookup(&method_id.1),
                    file_path.0.0,
                    start_offset,
                )
            }
            DataFlowNodeId::ThisAfterMethod(method_id) => format!(
                "$this in {} after {}",
                interner.lookup(&method_id.0),
                interner.lookup(&method_id.1)
            ),
            DataFlowNodeId::SpecializedThisAfterMethod(method_id, file_path, start_offset) => {
                format!(
                    "$this in {} after {}-{}:{}",
                    interner.lookup(&method_id.0),
                    interner.lookup(&method_id.1),
                    file_path.0.0,
                    start_offset,
                )
            }
            DataFlowNodeId::Symbol(id) => interner.lookup(id).to_string(),
            DataFlowNodeId::ShapeFieldAccess(type_name, key) => {
                format!("{}[{}]", interner.lookup(type_name), key)
            }
            DataFlowNodeId::Composition(file_path, start_offset, end_offset) => format!(
                "composition-{}:{}-{}",
                file_path.0.0, start_offset, end_offset
            ),
            DataFlowNodeId::ReferenceTo(functionlike_id) => {
                format!("fnref-{}", functionlike_id.to_string(interner))
            }
            DataFlowNodeId::ForInit(start_offset, end_offset) => {
                format!("for-init-{}-{}", start_offset, end_offset)
            }
            DataFlowNodeId::UnlabelledSink(file_path, start_offset, end_offset) => format!(
                "unlabelled-sink-{}:{}-{}",
                file_path.0.0, start_offset, end_offset
            ),
            DataFlowNodeId::InstanceMethodCall(file_path, start_offset, end_offset) => format!(
                "instance-method-call-{}:{}-{}",
                file_path.0.0, start_offset, end_offset
            ),
        }
    }

    pub fn to_label(&self, interner: &Interner) -> String {
        match self {
            DataFlowNodeId::String(str) | DataFlowNodeId::LocalString(str, ..) => str.clone(),
            DataFlowNodeId::Param(var_id, ..) | DataFlowNodeId::Var(var_id, ..) => {
                interner.lookup(&var_id.0).to_string()
            }
            DataFlowNodeId::VarNarrowedTo(var_id, symbol, ..) => {
                format!("{} narrowed to {}", var_id, interner.lookup(symbol),)
            }
            DataFlowNodeId::ArrayAssignment(..) => "array-assignment".to_string(),
            DataFlowNodeId::ArrayItem(key_value, ..) => {
                format!("array[{}]", key_value)
            }
            DataFlowNodeId::Return(..) => "return".to_string(),
            DataFlowNodeId::CallTo(functionlike_id)
            | DataFlowNodeId::SpecializedCallTo(functionlike_id, ..) => {
                format!("call to {}", functionlike_id.to_string(interner))
            }
            DataFlowNodeId::Property(classlike_name, property_name)
            | DataFlowNodeId::SpecializedProperty(classlike_name, property_name, ..) => format!(
                "{}::${}",
                interner.lookup(classlike_name),
                interner.lookup(property_name)
            ),

            DataFlowNodeId::FunctionLikeOut(functionlike_id, arg)
            | DataFlowNodeId::SpecializedFunctionLikeOut(functionlike_id, arg, ..) => {
                format!("out {}#{}", functionlike_id.to_string(interner), (arg + 1))
            }

            DataFlowNodeId::FunctionLikeArg(functionlike_id, arg)
            | DataFlowNodeId::SpecializedFunctionLikeArg(functionlike_id, arg, ..) => {
                format!("{}#{}", functionlike_id.to_string(interner), (arg + 1))
            }

            DataFlowNodeId::PropertyFetch(lhs_var_id, property_name, ..) => {
                format!(
                    "{}->{}",
                    interner.lookup(&lhs_var_id.0),
                    interner.lookup(property_name),
                )
            }

            DataFlowNodeId::ThisBeforeMethod(method_id)
            | DataFlowNodeId::SpecializedThisBeforeMethod(method_id, ..) => format!(
                "$this in {} before {}",
                interner.lookup(&method_id.0),
                interner.lookup(&method_id.1)
            ),

            DataFlowNodeId::ThisAfterMethod(method_id)
            | DataFlowNodeId::SpecializedThisAfterMethod(method_id, ..) => format!(
                "$this in {} after {}",
                interner.lookup(&method_id.0),
                interner.lookup(&method_id.1)
            ),

            DataFlowNodeId::Symbol(id) => interner.lookup(id).to_string(),
            DataFlowNodeId::ShapeFieldAccess(type_name, key) => {
                format!("{}[{}]", interner.lookup(type_name), key)
            }
            DataFlowNodeId::Composition(..) => "composition".to_string(),
            DataFlowNodeId::ReferenceTo(functionlike_id) => {
                format!("fnref-{}", functionlike_id.to_string(interner))
            }
            DataFlowNodeId::ForInit(start_offset, end_offset) => {
                format!("for-init-{}-{}", start_offset, end_offset)
            }
            DataFlowNodeId::UnlabelledSink(..) => panic!(),
            DataFlowNodeId::InstanceMethodCall(..) => "instance method call".to_string(),
        }
    }

    pub fn specialize(&self, file_path: FilePath, offset: u32) -> DataFlowNodeId {
        match self {
            DataFlowNodeId::CallTo(id) => DataFlowNodeId::SpecializedCallTo(*id, file_path, offset),
            DataFlowNodeId::FunctionLikeArg(functionlike_id, arg) => {
                DataFlowNodeId::SpecializedFunctionLikeArg(
                    *functionlike_id,
                    *arg,
                    file_path,
                    offset,
                )
            }
            DataFlowNodeId::FunctionLikeOut(functionlike_id, arg) => {
                DataFlowNodeId::SpecializedFunctionLikeOut(
                    *functionlike_id,
                    *arg,
                    file_path,
                    offset,
                )
            }
            DataFlowNodeId::ThisBeforeMethod(method_id) => {
                DataFlowNodeId::SpecializedThisBeforeMethod(*method_id, file_path, offset)
            }
            DataFlowNodeId::ThisAfterMethod(method_id) => {
                DataFlowNodeId::SpecializedThisAfterMethod(*method_id, file_path, offset)
            }
            _ => {
                panic!()
            }
        }
    }

    pub fn unspecialize(&self) -> (DataFlowNodeId, (FilePath, u32)) {
        match self {
            DataFlowNodeId::SpecializedCallTo(id, file_path, offset) => {
                (DataFlowNodeId::CallTo(*id), (*file_path, *offset))
            }
            DataFlowNodeId::SpecializedFunctionLikeArg(functionlike_id, arg, file_path, offset) => {
                (
                    DataFlowNodeId::FunctionLikeArg(*functionlike_id, *arg),
                    (*file_path, *offset),
                )
            }
            DataFlowNodeId::SpecializedFunctionLikeOut(functionlike_id, arg, file_path, offset) => {
                (
                    DataFlowNodeId::FunctionLikeOut(*functionlike_id, *arg),
                    (*file_path, *offset),
                )
            }
            DataFlowNodeId::SpecializedThisBeforeMethod(method_id, file_path, offset) => (
                DataFlowNodeId::ThisBeforeMethod(*method_id),
                (*file_path, *offset),
            ),
            DataFlowNodeId::SpecializedThisAfterMethod(method_id, file_path, offset) => (
                DataFlowNodeId::ThisAfterMethod(*method_id),
                (*file_path, *offset),
            ),
            _ => {
                panic!()
            }
        }
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct DataFlowNode {
    pub id: DataFlowNodeId,
    pub kind: DataFlowNodeKind,
}

impl PartialEq for DataFlowNode {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataFlowNodeKind {
    Vertex {
        pos: Option<HPos>,
        is_specialized: bool,
    },
    VariableUseSource {
        pos: HPos,
        kind: VariableSourceKind,
        pure: bool,
        has_parent_nodes: bool,
        has_await_call: bool,
        has_awaitable: bool,
        from_loop_init: bool,
    },
    VariableUseSink {
        pos: HPos,
    },
    ForLoopInit {
        var_id: VarId,
    },
    DataSource {
        pos: HPos,
        target_id: String,
    },
    TaintSource {
        pos: Option<HPos>,
        types: Vec<SourceType>,
    },
    TaintSink {
        pos: HPos,
        types: Vec<SinkType>,
    },
}

impl Hash for DataFlowNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl DataFlowNode {
    pub fn get_for_method_argument(
        functionlike_id: &FunctionLikeIdentifier,
        argument_offset: usize,
        arg_location: Option<HPos>,
        pos: Option<HPos>,
    ) -> Self {
        let arg_id = DataFlowNodeId::FunctionLikeArg(*functionlike_id, argument_offset as u8);

        let mut is_specialized = false;

        let mut id = arg_id.clone();

        if let Some(pos) = pos {
            is_specialized = true;
            id = DataFlowNodeId::SpecializedFunctionLikeArg(
                *functionlike_id,
                argument_offset as u8,
                pos.file_path,
                pos.start_offset,
            );
        }

        DataFlowNode {
            id,
            kind: DataFlowNodeKind::Vertex {
                pos: arg_location,
                is_specialized,
            },
        }
    }

    pub fn get_for_property(property_id: (StrId, StrId)) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::Property(property_id.0, property_id.1),
            kind: DataFlowNodeKind::Vertex {
                pos: None,
                is_specialized: false,
            },
        }
    }

    pub fn get_for_localized_property(
        property_id: (StrId, StrId),
        assignment_location: HPos,
    ) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::SpecializedProperty(
                property_id.0,
                property_id.1,
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_method_argument_out(
        functionlike_id: &FunctionLikeIdentifier,
        argument_offset: usize,
        arg_location: Option<HPos>,
        pos: Option<HPos>,
    ) -> Self {
        let mut arg_id = DataFlowNodeId::FunctionLikeOut(*functionlike_id, argument_offset as u8);

        let mut is_specialized = false;

        if let Some(pos) = pos {
            is_specialized = true;
            arg_id = DataFlowNodeId::SpecializedFunctionLikeOut(
                *functionlike_id,
                argument_offset as u8,
                pos.file_path,
                pos.start_offset,
            );
        }

        DataFlowNode {
            id: arg_id,
            kind: DataFlowNodeKind::Vertex {
                pos: arg_location,
                is_specialized,
            },
        }
    }

    pub fn get_for_this_before_method(
        method_id: &MethodIdentifier,
        method_location: Option<HPos>,
        pos: Option<HPos>,
    ) -> Self {
        let label = DataFlowNodeId::ThisBeforeMethod(*method_id);

        let mut is_specialized = false;
        let mut id = label.clone();

        if let Some(pos) = pos {
            is_specialized = true;
            id = DataFlowNodeId::SpecializedThisBeforeMethod(
                *method_id,
                pos.file_path,
                pos.start_offset,
            );
        }

        DataFlowNode {
            id,
            kind: DataFlowNodeKind::Vertex {
                pos: method_location,
                is_specialized,
            },
        }
    }

    pub fn get_for_this_after_method(
        method_id: &MethodIdentifier,
        method_location: Option<HPos>,
        pos: Option<HPos>,
    ) -> Self {
        let label = DataFlowNodeId::ThisAfterMethod(*method_id);

        let mut is_specialized = false;
        let mut id = label.clone();

        if let Some(pos) = pos {
            is_specialized = true;
            id = DataFlowNodeId::SpecializedThisAfterMethod(
                *method_id,
                pos.file_path,
                pos.start_offset,
            );
        }

        DataFlowNode {
            id,
            kind: DataFlowNodeKind::Vertex {
                pos: method_location,
                is_specialized,
            },
        }
    }

    pub fn get_for_lvar(var_id: VarId, assignment_location: HPos) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::Var(
                var_id,
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_array_assignment(assignment_location: HPos) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::ArrayAssignment(
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_return_expr(assignment_location: HPos) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::Return(
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_array_item(key_value: String, assignment_location: HPos) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::ArrayItem(
                key_value.clone(),
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_local_string(var_id: String, assignment_location: HPos) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::LocalString(
                var_id,
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_instance_method_call(assignment_location: HPos) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::InstanceMethodCall(
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_local_property_fetch(
        lhs_var_id: VarId,
        property_name: StrId,
        assignment_location: HPos,
    ) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::PropertyFetch(
                lhs_var_id,
                property_name,
                assignment_location.file_path,
                assignment_location.start_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_narrowing(
        var_id: String,
        narrowed_symbol: &StrId,
        assignment_location: HPos,
    ) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::VarNarrowedTo(
                var_id,
                *narrowed_symbol,
                assignment_location.file_path,
                assignment_location.start_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_type(type_name: &StrId, def_location: HPos) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::Symbol(*type_name),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(def_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_call(
        functionlike_id: FunctionLikeIdentifier,
        assignment_location: HPos,
    ) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::SpecializedCallTo(
                functionlike_id,
                assignment_location.file_path,
                assignment_location.start_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_composition(assignment_location: HPos) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::Composition(
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::Vertex {
                pos: Some(assignment_location),
                is_specialized: false,
            },
        }
    }

    pub fn get_for_unlabelled_sink(assignment_location: HPos) -> Self {
        Self {
            id: DataFlowNodeId::UnlabelledSink(
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::VariableUseSink {
                pos: assignment_location,
            },
        }
    }

    pub fn get_for_variable_sink(label: VarId, assignment_location: HPos) -> Self {
        Self {
            id: DataFlowNodeId::Var(
                label,
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::VariableUseSink {
                pos: assignment_location,
            },
        }
    }

    pub fn get_for_variable_source(
        kind: VariableSourceKind,
        label: VarId,
        assignment_location: HPos,
        pure: bool,
        has_parent_nodes: bool,
        has_awaitable: bool,
        has_await_call: bool,
        from_loop_init: bool,
    ) -> Self {
        Self {
            id: DataFlowNodeId::Var(
                label,
                assignment_location.file_path,
                assignment_location.start_offset,
                assignment_location.end_offset,
            ),
            kind: DataFlowNodeKind::VariableUseSource {
                pos: assignment_location,
                kind,
                pure,
                has_awaitable,
                has_await_call,
                has_parent_nodes,
                from_loop_init,
            },
        }
    }

    pub fn get_for_method_return(
        functionlike_id: &FunctionLikeIdentifier,
        pos: Option<HPos>,
        specialization_location: Option<HPos>,
    ) -> Self {
        let mut is_specialized = false;

        let mut id = DataFlowNodeId::CallTo(*functionlike_id);

        if let Some(specialization_location) = specialization_location {
            is_specialized = true;

            id = DataFlowNodeId::SpecializedCallTo(
                *functionlike_id,
                specialization_location.file_path,
                specialization_location.start_offset,
            );
        }

        DataFlowNode {
            id,
            kind: DataFlowNodeKind::Vertex {
                pos,
                is_specialized,
            },
        }
    }

    pub fn get_for_method_reference(
        functionlike_id: &FunctionLikeIdentifier,
        pos: Option<HPos>,
    ) -> Self {
        DataFlowNode {
            id: DataFlowNodeId::ReferenceTo(*functionlike_id),
            kind: DataFlowNodeKind::Vertex {
                pos,
                is_specialized: false,
            },
        }
    }

    #[inline]
    pub fn get_pos(&self) -> Option<HPos> {
        match &self.kind {
            DataFlowNodeKind::Vertex { pos, .. } | DataFlowNodeKind::TaintSource { pos, .. } => {
                *pos
            }
            DataFlowNodeKind::TaintSink { pos, .. }
            | DataFlowNodeKind::VariableUseSource { pos, .. }
            | DataFlowNodeKind::DataSource { pos, .. } => Some(*pos),
            DataFlowNodeKind::ForLoopInit { .. } | DataFlowNodeKind::VariableUseSink { .. } => None,
        }
    }
}
