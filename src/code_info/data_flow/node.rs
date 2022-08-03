use crate::{code_location::HPos, taint::TaintType};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    TaintSource,
    TaintSink,
    Default,
    PrivateParam,
    NonPrivateParam,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataFlowNode {
    pub kind: NodeKind,
    pub id: String,
    pub unspecialized_id: Option<String>,
    pub label: String,
    pub pos: Option<HPos>,
    pub specialization_key: Option<String>,
    pub taints: Option<HashSet<TaintType>>,
}

impl DataFlowNode {
    pub fn new(
        kind: NodeKind,
        id: String,
        label: String,
        pos: Option<HPos>,
        specialization_key: Option<String>,
        taints: Option<HashSet<TaintType>>,
    ) -> Self {
        let mut id = id;
        let mut unspecialized_id = None;

        if let Some(specialization_key) = &specialization_key {
            unspecialized_id = Some(id.clone());
            id += "-";
            id += specialization_key.as_str();
        }

        Self {
            kind,
            id,
            unspecialized_id,
            label,
            pos,
            specialization_key,
            taints,
        }
    }

    pub fn get_for_method_argument(
        kind: NodeKind,
        method_id: String,
        argument_offset: usize,
        arg_location: Option<HPos>,
        pos: Option<HPos>,
    ) -> Self {
        let arg_id = method_id.clone() + "#" + (argument_offset + 1).to_string().as_str();

        let mut specialization_key = None;

        if let Some(pos) = pos {
            specialization_key = Some(format!("{}:{}", pos.file_path, pos.start_offset));
        }

        Self::new(
            kind,
            arg_id.clone(),
            arg_id,
            arg_location,
            specialization_key,
            None,
        )
    }

    pub fn get_for_assignment(
        var_id: String,
        assignment_location: HPos,
        specialization_key: Option<String>,
    ) -> Self {
        let id = format!(
            "{}-{}:{}-{}",
            var_id,
            assignment_location.file_path,
            assignment_location.start_offset,
            assignment_location.end_offset
        );

        Self::new(
            NodeKind::Default,
            id,
            var_id,
            Some(assignment_location),
            specialization_key,
            None,
        )
    }

    pub fn get_for_param(var_id: String, kind: NodeKind, assignment_location: HPos) -> Self {
        let id = format!(
            "{}-{}:{}-{}",
            var_id,
            assignment_location.file_path,
            assignment_location.start_offset,
            assignment_location.end_offset
        );

        Self::new(kind, id, var_id, Some(assignment_location), None, None)
    }

    pub fn get_for_variable_use(label: String, assignment_location: HPos) -> Self {
        let id = format!(
            "{}-{}:{}-{}",
            label,
            assignment_location.file_path,
            assignment_location.start_offset,
            assignment_location.end_offset
        );

        Self::new(
            NodeKind::Default,
            id,
            label,
            Some(assignment_location),
            None,
            None,
        )
    }

    pub fn get_for_method_return(
        kind: NodeKind,
        method_id: String,
        pos: Option<HPos>,
        function_location: Option<HPos>,
    ) -> Self {
        let mut specialization_key = None;

        if let Some(function_location) = function_location {
            specialization_key = Some(
                (*function_location.file_path).clone()
                    + ":"
                    + function_location.start_offset.to_string().as_str(),
            );
        }

        Self::new(
            kind,
            method_id.clone(),
            format!("{}()", method_id),
            pos,
            specialization_key,
            None,
        )
    }

    pub fn get_for_method_reference(method_id: String, pos: HPos) -> Self {
        Self::new(
            NodeKind::Default,
            format!("fnref-{}", method_id),
            format!("{}()", method_id),
            Some(pos),
            None,
            None,
        )
    }
}
