//! Binary serialization for the hakana protocol.
//!
//! Message format:
//! ```text
//! ┌──────────────┬──────────────┬─────────────────┐
//! │ Length (u32) │ Type (u8)    │ Payload (bytes) │
//! └──────────────┴──────────────┴─────────────────┘
//! ```

use std::io::{self, Read, Write};

use crate::types::*;

/// Protocol errors.
#[derive(Debug)]
pub enum ProtocolError {
    /// I/O error during read/write.
    Io(io::Error),
    /// Invalid message type.
    InvalidMessageType(u8),
    /// Invalid error code.
    InvalidErrorCode(u32),
    /// Invalid file change status.
    InvalidFileChangeStatus(u8),
    /// Unexpected end of data.
    UnexpectedEof,
    /// String is not valid UTF-8.
    InvalidUtf8,
    /// Message too large.
    MessageTooLarge(u32),
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {}", e),
            Self::InvalidMessageType(t) => write!(f, "Invalid message type: 0x{:02x}", t),
            Self::InvalidErrorCode(c) => write!(f, "Invalid error code: {}", c),
            Self::InvalidFileChangeStatus(s) => write!(f, "Invalid file change status: {}", s),
            Self::UnexpectedEof => write!(f, "Unexpected end of data"),
            Self::InvalidUtf8 => write!(f, "Invalid UTF-8 string"),
            Self::MessageTooLarge(size) => write!(f, "Message too large: {} bytes", size),
        }
    }
}

impl std::error::Error for ProtocolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ProtocolError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Maximum message size (256 MB).
const MAX_MESSAGE_SIZE: u32 = 256 * 1024 * 1024;

/// Trait for serializing protocol types to bytes.
pub trait Serialize {
    /// Serialize to a byte vector.
    fn serialize(&self, buf: &mut Vec<u8>);
}

/// Trait for deserializing protocol types from bytes.
pub trait Deserialize: Sized {
    /// Deserialize from a byte slice, returning the value and remaining bytes.
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError>;
}

// Primitive serialization helpers

fn write_u8(buf: &mut Vec<u8>, v: u8) {
    buf.push(v);
}

fn write_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_u64(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_bool(buf: &mut Vec<u8>, v: bool) {
    buf.push(if v { 1 } else { 0 });
}

fn write_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    write_u32(buf, bytes.len() as u32);
    buf.extend_from_slice(bytes);
}

fn write_option_string(buf: &mut Vec<u8>, s: &Option<String>) {
    match s {
        Some(s) => {
            write_bool(buf, true);
            write_string(buf, s);
        }
        None => {
            write_bool(buf, false);
        }
    }
}

fn read_u8(data: &[u8]) -> Result<(u8, &[u8]), ProtocolError> {
    if data.is_empty() {
        return Err(ProtocolError::UnexpectedEof);
    }
    Ok((data[0], &data[1..]))
}

fn read_u16(data: &[u8]) -> Result<(u16, &[u8]), ProtocolError> {
    if data.len() < 2 {
        return Err(ProtocolError::UnexpectedEof);
    }
    let v = u16::from_le_bytes([data[0], data[1]]);
    Ok((v, &data[2..]))
}

fn read_u32(data: &[u8]) -> Result<(u32, &[u8]), ProtocolError> {
    if data.len() < 4 {
        return Err(ProtocolError::UnexpectedEof);
    }
    let v = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    Ok((v, &data[4..]))
}

fn read_u64(data: &[u8]) -> Result<(u64, &[u8]), ProtocolError> {
    if data.len() < 8 {
        return Err(ProtocolError::UnexpectedEof);
    }
    let v = u64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ]);
    Ok((v, &data[8..]))
}

fn read_bool(data: &[u8]) -> Result<(bool, &[u8]), ProtocolError> {
    let (v, rest) = read_u8(data)?;
    Ok((v != 0, rest))
}

fn read_string(data: &[u8]) -> Result<(String, &[u8]), ProtocolError> {
    let (len, rest) = read_u32(data)?;
    let len = len as usize;
    if rest.len() < len {
        return Err(ProtocolError::UnexpectedEof);
    }
    let s = std::str::from_utf8(&rest[..len]).map_err(|_| ProtocolError::InvalidUtf8)?;
    Ok((s.to_string(), &rest[len..]))
}

fn read_option_string(data: &[u8]) -> Result<(Option<String>, &[u8]), ProtocolError> {
    let (present, rest) = read_bool(data)?;
    if present {
        let (s, rest) = read_string(rest)?;
        Ok((Some(s), rest))
    } else {
        Ok((None, rest))
    }
}

// FileChangeStatus

impl Serialize for FileChangeStatus {
    fn serialize(&self, buf: &mut Vec<u8>) {
        write_u8(buf, *self as u8);
    }
}

impl Deserialize for FileChangeStatus {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (v, rest) = read_u8(data)?;
        let status = match v {
            0 => Self::Added,
            1 => Self::Modified,
            2 => Self::Deleted,
            _ => return Err(ProtocolError::InvalidFileChangeStatus(v)),
        };
        Ok((status, rest))
    }
}

// FileChange

impl Serialize for FileChange {
    fn serialize(&self, buf: &mut Vec<u8>) {
        write_string(buf, &self.path);
        self.status.serialize(buf);
    }
}

impl Deserialize for FileChange {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (path, rest) = read_string(data)?;
        let (status, rest) = FileChangeStatus::deserialize(rest)?;
        Ok((Self { path, status }, rest))
    }
}

// AnalyzeRequest

impl Serialize for AnalyzeRequest {
    fn serialize(&self, buf: &mut Vec<u8>) {
        write_option_string(buf, &self.filter);
        write_bool(buf, self.find_unused_expressions);
        write_bool(buf, self.find_unused_definitions);
        write_u32(buf, self.file_changes.len() as u32);
        for change in &self.file_changes {
            change.serialize(buf);
        }
        write_bool(buf, self.full_rescan);
        // Serialize allowed_issues
        match &self.allowed_issues {
            Some(issues) => {
                write_bool(buf, true);
                write_u32(buf, issues.len() as u32);
                for issue in issues {
                    write_string(buf, issue);
                }
            }
            None => {
                write_bool(buf, false);
            }
        }
    }
}

impl Deserialize for AnalyzeRequest {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (filter, rest) = read_option_string(data)?;
        let (find_unused_expressions, rest) = read_bool(rest)?;
        let (find_unused_definitions, rest) = read_bool(rest)?;
        let (changes_len, mut rest) = read_u32(rest)?;
        let mut file_changes = Vec::with_capacity(changes_len as usize);
        for _ in 0..changes_len {
            let (change, r) = FileChange::deserialize(rest)?;
            file_changes.push(change);
            rest = r;
        }
        let (full_rescan, rest) = read_bool(rest)?;
        let (has_allowed, rest) = read_bool(rest)?;
        let (allowed_issues, rest) = if has_allowed {
            let (count, mut rest) = read_u32(rest)?;
            let mut issues = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let (issue, r) = read_string(rest)?;
                issues.push(issue);
                rest = r;
            }
            (Some(issues), rest)
        } else {
            (None, rest)
        };
        Ok((
            Self {
                filter,
                find_unused_expressions,
                find_unused_definitions,
                file_changes,
                full_rescan,
                allowed_issues,
            },
            rest,
        ))
    }
}

// ProtocolIssue

impl Serialize for ProtocolIssue {
    fn serialize(&self, buf: &mut Vec<u8>) {
        write_string(buf, &self.kind);
        write_string(buf, &self.description);
        write_string(buf, &self.file_path);
        write_u32(buf, self.start_line);
        write_u16(buf, self.start_column);
        write_u32(buf, self.end_line);
        write_u16(buf, self.end_column);
        write_option_string(buf, &self.suggestion);
    }
}

impl Deserialize for ProtocolIssue {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (kind, rest) = read_string(data)?;
        let (description, rest) = read_string(rest)?;
        let (file_path, rest) = read_string(rest)?;
        let (start_line, rest) = read_u32(rest)?;
        let (start_column, rest) = read_u16(rest)?;
        let (end_line, rest) = read_u32(rest)?;
        let (end_column, rest) = read_u16(rest)?;
        let (suggestion, rest) = read_option_string(rest)?;
        Ok((
            Self {
                kind,
                description,
                file_path,
                start_line,
                start_column,
                end_line,
                end_column,
                suggestion,
            },
            rest,
        ))
    }
}

// AnalyzeResponse

impl Serialize for AnalyzeResponse {
    fn serialize(&self, buf: &mut Vec<u8>) {
        write_bool(buf, self.success);
        write_u32(buf, self.issues.len() as u32);
        for issue in &self.issues {
            issue.serialize(buf);
        }
        write_u64(buf, self.scan_time_ms);
        write_u64(buf, self.analysis_time_ms);
        write_u32(buf, self.files_analyzed);
        write_u32(buf, self.files_with_issues);
    }
}

impl Deserialize for AnalyzeResponse {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (success, rest) = read_bool(data)?;
        let (issues_len, mut rest) = read_u32(rest)?;
        let mut issues = Vec::with_capacity(issues_len as usize);
        for _ in 0..issues_len {
            let (issue, r) = ProtocolIssue::deserialize(rest)?;
            issues.push(issue);
            rest = r;
        }
        let (scan_time_ms, rest) = read_u64(rest)?;
        let (analysis_time_ms, rest) = read_u64(rest)?;
        let (files_analyzed, rest) = read_u32(rest)?;
        let (files_with_issues, rest) = read_u32(rest)?;
        Ok((
            Self {
                success,
                issues,
                scan_time_ms,
                analysis_time_ms,
                files_analyzed,
                files_with_issues,
            },
            rest,
        ))
    }
}

// SecurityCheckRequest

impl Serialize for SecurityCheckRequest {
    fn serialize(&self, buf: &mut Vec<u8>) {
        match self.max_depth {
            Some(d) => {
                write_bool(buf, true);
                write_u32(buf, d);
            }
            None => write_bool(buf, false),
        }
        write_option_string(buf, &self.filter);
    }
}

impl Deserialize for SecurityCheckRequest {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (has_depth, rest) = read_bool(data)?;
        let (max_depth, rest) = if has_depth {
            let (d, r) = read_u32(rest)?;
            (Some(d), r)
        } else {
            (None, rest)
        };
        let (filter, rest) = read_option_string(rest)?;
        Ok((Self { max_depth, filter }, rest))
    }
}

// SecurityCheckResponse

impl Serialize for SecurityCheckResponse {
    fn serialize(&self, buf: &mut Vec<u8>) {
        write_bool(buf, self.success);
        write_u32(buf, self.issues.len() as u32);
        for issue in &self.issues {
            issue.serialize(buf);
        }
        write_u32(buf, self.taint_flows_found);
        write_u64(buf, self.analysis_time_ms);
    }
}

impl Deserialize for SecurityCheckResponse {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (success, rest) = read_bool(data)?;
        let (issues_len, mut rest) = read_u32(rest)?;
        let mut issues = Vec::with_capacity(issues_len as usize);
        for _ in 0..issues_len {
            let (issue, r) = ProtocolIssue::deserialize(rest)?;
            issues.push(issue);
            rest = r;
        }
        let (taint_flows_found, rest) = read_u32(rest)?;
        let (analysis_time_ms, rest) = read_u64(rest)?;
        Ok((
            Self {
                success,
                issues,
                taint_flows_found,
                analysis_time_ms,
            },
            rest,
        ))
    }
}

// GotoDefinitionRequest

impl Serialize for GotoDefinitionRequest {
    fn serialize(&self, buf: &mut Vec<u8>) {
        write_string(buf, &self.file_path);
        write_u32(buf, self.line);
        write_u32(buf, self.column);
    }
}

impl Deserialize for GotoDefinitionRequest {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (file_path, rest) = read_string(data)?;
        let (line, rest) = read_u32(rest)?;
        let (column, rest) = read_u32(rest)?;
        Ok((
            Self {
                file_path,
                line,
                column,
            },
            rest,
        ))
    }
}

// GotoDefinitionResponse

impl Serialize for GotoDefinitionResponse {
    fn serialize(&self, buf: &mut Vec<u8>) {
        write_bool(buf, self.found);
        write_option_string(buf, &self.file_path);
        match self.start_line {
            Some(v) => {
                write_bool(buf, true);
                write_u32(buf, v);
            }
            None => write_bool(buf, false),
        }
        match self.start_column {
            Some(v) => {
                write_bool(buf, true);
                write_u16(buf, v);
            }
            None => write_bool(buf, false),
        }
        match self.end_line {
            Some(v) => {
                write_bool(buf, true);
                write_u32(buf, v);
            }
            None => write_bool(buf, false),
        }
        match self.end_column {
            Some(v) => {
                write_bool(buf, true);
                write_u16(buf, v);
            }
            None => write_bool(buf, false),
        }
    }
}

impl Deserialize for GotoDefinitionResponse {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (found, rest) = read_bool(data)?;
        let (file_path, rest) = read_option_string(rest)?;
        let (has_start_line, rest) = read_bool(rest)?;
        let (start_line, rest) = if has_start_line {
            let (v, r) = read_u32(rest)?;
            (Some(v), r)
        } else {
            (None, rest)
        };
        let (has_start_column, rest) = read_bool(rest)?;
        let (start_column, rest) = if has_start_column {
            let (v, r) = read_u16(rest)?;
            (Some(v), r)
        } else {
            (None, rest)
        };
        let (has_end_line, rest) = read_bool(rest)?;
        let (end_line, rest) = if has_end_line {
            let (v, r) = read_u32(rest)?;
            (Some(v), r)
        } else {
            (None, rest)
        };
        let (has_end_column, rest) = read_bool(rest)?;
        let (end_column, rest) = if has_end_column {
            let (v, r) = read_u16(rest)?;
            (Some(v), r)
        } else {
            (None, rest)
        };
        Ok((
            Self {
                found,
                file_path,
                start_line,
                start_column,
                end_line,
                end_column,
            },
            rest,
        ))
    }
}

// FindReferencesRequest

impl Serialize for FindReferencesRequest {
    fn serialize(&self, buf: &mut Vec<u8>) {
        write_string(buf, &self.file_path);
        write_u32(buf, self.line);
        write_u32(buf, self.column);
    }
}

impl Deserialize for FindReferencesRequest {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (file_path, rest) = read_string(data)?;
        let (line, rest) = read_u32(rest)?;
        let (column, rest) = read_u32(rest)?;
        Ok((
            Self {
                file_path,
                line,
                column,
            },
            rest,
        ))
    }
}

// ReferenceLocation

impl Serialize for ReferenceLocation {
    fn serialize(&self, buf: &mut Vec<u8>) {
        write_string(buf, &self.file_path);
        write_u32(buf, self.line);
        write_u16(buf, self.column);
    }
}

impl Deserialize for ReferenceLocation {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (file_path, rest) = read_string(data)?;
        let (line, rest) = read_u32(rest)?;
        let (column, rest) = read_u16(rest)?;
        Ok((
            Self {
                file_path,
                line,
                column,
            },
            rest,
        ))
    }
}

// FindReferencesResponse

impl Serialize for FindReferencesResponse {
    fn serialize(&self, buf: &mut Vec<u8>) {
        write_u32(buf, self.references.len() as u32);
        for loc in &self.references {
            loc.serialize(buf);
        }
    }
}

impl Deserialize for FindReferencesResponse {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (len, mut rest) = read_u32(data)?;
        let mut references = Vec::with_capacity(len as usize);
        for _ in 0..len {
            let (loc, r) = ReferenceLocation::deserialize(rest)?;
            references.push(loc);
            rest = r;
        }
        Ok((Self { references }, rest))
    }
}

// GetIssuesRequest

impl Serialize for GetIssuesRequest {
    fn serialize(&self, buf: &mut Vec<u8>) {
        write_option_string(buf, &self.filter);
        write_bool(buf, self.find_unused_expressions);
        write_bool(buf, self.find_unused_definitions);
    }
}

impl Deserialize for GetIssuesRequest {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (filter, rest) = read_option_string(data)?;
        let (find_unused_expressions, rest) = read_bool(rest)?;
        let (find_unused_definitions, rest) = read_bool(rest)?;
        Ok((
            Self {
                filter,
                find_unused_expressions,
                find_unused_definitions,
            },
            rest,
        ))
    }
}

// GetIssuesResponse

impl Serialize for GetIssuesResponse {
    fn serialize(&self, buf: &mut Vec<u8>) {
        write_bool(buf, self.analysis_complete);
        write_u32(buf, self.issues.len() as u32);
        for issue in &self.issues {
            issue.serialize(buf);
        }
        write_u32(buf, self.files_analyzed);
        write_u32(buf, self.total_files);
        write_string(buf, &self.phase);
        write_u8(buf, self.progress_percent);
    }
}

impl Deserialize for GetIssuesResponse {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (analysis_complete, rest) = read_bool(data)?;
        let (issues_len, mut rest) = read_u32(rest)?;
        let mut issues = Vec::with_capacity(issues_len as usize);
        for _ in 0..issues_len {
            let (issue, r) = ProtocolIssue::deserialize(rest)?;
            issues.push(issue);
            rest = r;
        }
        let (files_analyzed, rest) = read_u32(rest)?;
        let (total_files, rest) = read_u32(rest)?;
        let (phase, rest) = read_string(rest)?;
        let (progress_percent, rest) = read_u8(rest)?;
        Ok((
            Self {
                analysis_complete,
                issues,
                files_analyzed,
                total_files,
                phase,
                progress_percent,
            },
            rest,
        ))
    }
}

// StatusRequest (empty)

impl Serialize for StatusRequest {
    fn serialize(&self, _buf: &mut Vec<u8>) {
        // No payload
    }
}

impl Deserialize for StatusRequest {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        Ok((Self, data))
    }
}

// StatusResponse

impl Serialize for StatusResponse {
    fn serialize(&self, buf: &mut Vec<u8>) {
        write_bool(buf, self.ready);
        write_u32(buf, self.files_count);
        write_u32(buf, self.symbols_count);
        write_u64(buf, self.uptime_secs);
        write_bool(buf, self.analysis_in_progress);
        write_u32(buf, self.pending_requests);
        write_string(buf, &self.project_root);
    }
}

impl Deserialize for StatusResponse {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (ready, rest) = read_bool(data)?;
        let (files_count, rest) = read_u32(rest)?;
        let (symbols_count, rest) = read_u32(rest)?;
        let (uptime_secs, rest) = read_u64(rest)?;
        let (analysis_in_progress, rest) = read_bool(rest)?;
        let (pending_requests, rest) = read_u32(rest)?;
        let (project_root, rest) = read_string(rest)?;
        Ok((
            Self {
                ready,
                files_count,
                symbols_count,
                uptime_secs,
                analysis_in_progress,
                pending_requests,
                project_root,
            },
            rest,
        ))
    }
}

// ShutdownRequest (empty)

impl Serialize for ShutdownRequest {
    fn serialize(&self, _buf: &mut Vec<u8>) {
        // No payload
    }
}

impl Deserialize for ShutdownRequest {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        Ok((Self, data))
    }
}

// AckResponse (empty)

impl Serialize for AckResponse {
    fn serialize(&self, _buf: &mut Vec<u8>) {
        // No payload
    }
}

impl Deserialize for AckResponse {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        Ok((Self, data))
    }
}

// ErrorCode

impl Serialize for ErrorCode {
    fn serialize(&self, buf: &mut Vec<u8>) {
        write_u32(buf, *self as u32);
    }
}

impl Deserialize for ErrorCode {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (v, rest) = read_u32(data)?;
        let code = Self::try_from(v).map_err(ProtocolError::InvalidErrorCode)?;
        Ok((code, rest))
    }
}

// ErrorResponse

impl Serialize for ErrorResponse {
    fn serialize(&self, buf: &mut Vec<u8>) {
        self.code.serialize(buf);
        write_string(buf, &self.message);
    }
}

impl Deserialize for ErrorResponse {
    fn deserialize(data: &[u8]) -> Result<(Self, &[u8]), ProtocolError> {
        let (code, rest) = ErrorCode::deserialize(data)?;
        let (message, rest) = read_string(rest)?;
        Ok((Self { code, message }, rest))
    }
}

// Message (envelope)

impl Serialize for Message {
    fn serialize(&self, buf: &mut Vec<u8>) {
        match self {
            Self::Analyze(req) => req.serialize(buf),
            Self::SecurityCheck(req) => req.serialize(buf),
            Self::GotoDefinition(req) => req.serialize(buf),
            Self::FindReferences(req) => req.serialize(buf),
            Self::FileChanged(changes) => {
                write_u32(buf, changes.len() as u32);
                for change in changes {
                    change.serialize(buf);
                }
            }
            Self::GetIssues(req) => req.serialize(buf),
            Self::Status(req) => req.serialize(buf),
            Self::Shutdown(req) => req.serialize(buf),
            Self::AnalyzeResult(res) => res.serialize(buf),
            Self::SecurityCheckResult(res) => res.serialize(buf),
            Self::GotoDefinitionResult(res) => res.serialize(buf),
            Self::FindReferencesResult(res) => res.serialize(buf),
            Self::GetIssuesResult(res) => res.serialize(buf),
            Self::StatusResult(res) => res.serialize(buf),
            Self::Ack(res) => res.serialize(buf),
            Self::Error(res) => res.serialize(buf),
        }
    }
}

impl Message {
    /// Deserialize a message given its type.
    pub fn deserialize_with_type(
        msg_type: MessageType,
        data: &[u8],
    ) -> Result<Self, ProtocolError> {
        let msg = match msg_type {
            MessageType::AnalyzeRequest => {
                let (req, _) = AnalyzeRequest::deserialize(data)?;
                Self::Analyze(req)
            }
            MessageType::SecurityCheckRequest => {
                let (req, _) = SecurityCheckRequest::deserialize(data)?;
                Self::SecurityCheck(req)
            }
            MessageType::GotoDefinitionRequest => {
                let (req, _) = GotoDefinitionRequest::deserialize(data)?;
                Self::GotoDefinition(req)
            }
            MessageType::FindReferencesRequest => {
                let (req, _) = FindReferencesRequest::deserialize(data)?;
                Self::FindReferences(req)
            }
            MessageType::FileChangedNotification => {
                let (len, mut rest) = read_u32(data)?;
                let mut changes = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    let (change, r) = FileChange::deserialize(rest)?;
                    changes.push(change);
                    rest = r;
                }
                Self::FileChanged(changes)
            }
            MessageType::GetIssuesRequest => {
                let (req, _) = GetIssuesRequest::deserialize(data)?;
                Self::GetIssues(req)
            }
            MessageType::StatusRequest => {
                let (req, _) = StatusRequest::deserialize(data)?;
                Self::Status(req)
            }
            MessageType::ShutdownRequest => {
                let (req, _) = ShutdownRequest::deserialize(data)?;
                Self::Shutdown(req)
            }
            MessageType::AnalyzeResponse => {
                let (res, _) = AnalyzeResponse::deserialize(data)?;
                Self::AnalyzeResult(res)
            }
            MessageType::SecurityCheckResponse => {
                let (res, _) = SecurityCheckResponse::deserialize(data)?;
                Self::SecurityCheckResult(res)
            }
            MessageType::GotoDefinitionResponse => {
                let (res, _) = GotoDefinitionResponse::deserialize(data)?;
                Self::GotoDefinitionResult(res)
            }
            MessageType::FindReferencesResponse => {
                let (res, _) = FindReferencesResponse::deserialize(data)?;
                Self::FindReferencesResult(res)
            }
            MessageType::GetIssuesResponse => {
                let (res, _) = GetIssuesResponse::deserialize(data)?;
                Self::GetIssuesResult(res)
            }
            MessageType::StatusResponse => {
                let (res, _) = StatusResponse::deserialize(data)?;
                Self::StatusResult(res)
            }
            MessageType::AckResponse => {
                let (res, _) = AckResponse::deserialize(data)?;
                Self::Ack(res)
            }
            MessageType::ErrorResponse => {
                let (res, _) = ErrorResponse::deserialize(data)?;
                Self::Error(res)
            }
        };
        Ok(msg)
    }
}

/// Encode a message into a framed byte vector (length + type + payload).
pub fn encode_message(msg: &Message) -> Vec<u8> {
    let mut payload = Vec::new();
    msg.serialize(&mut payload);

    let mut frame = Vec::with_capacity(5 + payload.len());
    // Length prefix (4 bytes) = type (1 byte) + payload length
    let total_len = 1 + payload.len() as u32;
    frame.extend_from_slice(&total_len.to_le_bytes());
    // Message type
    frame.push(msg.message_type() as u8);
    // Payload
    frame.extend_from_slice(&payload);

    frame
}

/// Decode a message from a framed byte slice.
/// Returns the message and the remaining bytes.
pub fn decode_message(data: &[u8]) -> Result<(Message, &[u8]), ProtocolError> {
    if data.len() < 5 {
        return Err(ProtocolError::UnexpectedEof);
    }

    // Read length prefix
    let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if len > MAX_MESSAGE_SIZE {
        return Err(ProtocolError::MessageTooLarge(len));
    }

    let frame_len = len as usize;
    if data.len() < 4 + frame_len {
        return Err(ProtocolError::UnexpectedEof);
    }

    // Read message type
    let msg_type_byte = data[4];
    let msg_type =
        MessageType::try_from(msg_type_byte).map_err(ProtocolError::InvalidMessageType)?;

    // Read payload
    let payload = &data[5..4 + frame_len];
    let msg = Message::deserialize_with_type(msg_type, payload)?;

    Ok((msg, &data[4 + frame_len..]))
}

/// Read a complete message from a reader.
pub fn read_message<R: Read>(reader: &mut R) -> Result<Message, ProtocolError> {
    // Read length prefix
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf);

    if len > MAX_MESSAGE_SIZE {
        return Err(ProtocolError::MessageTooLarge(len));
    }

    // Read the rest of the frame
    let mut frame = vec![0u8; len as usize];
    reader.read_exact(&mut frame)?;

    // Parse message type
    if frame.is_empty() {
        return Err(ProtocolError::UnexpectedEof);
    }
    let msg_type_byte = frame[0];
    let msg_type =
        MessageType::try_from(msg_type_byte).map_err(ProtocolError::InvalidMessageType)?;

    // Parse payload
    let payload = &frame[1..];
    Message::deserialize_with_type(msg_type, payload)
}

/// Write a message to a writer.
pub fn write_message<W: Write>(writer: &mut W, msg: &Message) -> Result<(), ProtocolError> {
    let frame = encode_message(msg);
    writer.write_all(&frame)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_analyze_request() {
        let req = AnalyzeRequest {
            filter: Some("/src/".to_string()),
            find_unused_expressions: true,
            find_unused_definitions: false,
            file_changes: vec![
                FileChange {
                    path: "/src/foo.hack".to_string(),
                    status: FileChangeStatus::Modified,
                },
                FileChange {
                    path: "/src/bar.hack".to_string(),
                    status: FileChangeStatus::Added,
                },
            ],
            full_rescan: false,
            allowed_issues: Some(vec!["UnusedVariable".to_string()]),
        };

        let msg = Message::Analyze(req.clone());
        let encoded = encode_message(&msg);
        let (decoded, rest) = decode_message(&encoded).unwrap();
        assert!(rest.is_empty());

        if let Message::Analyze(decoded_req) = decoded {
            assert_eq!(decoded_req.filter, req.filter);
            assert_eq!(
                decoded_req.find_unused_expressions,
                req.find_unused_expressions
            );
            assert_eq!(decoded_req.file_changes.len(), req.file_changes.len());
        } else {
            panic!("Expected AnalyzeRequest");
        }
    }

    #[test]
    fn test_roundtrip_analyze_response() {
        let res = AnalyzeResponse {
            success: true,
            issues: vec![ProtocolIssue {
                kind: "UnusedVariable".to_string(),
                description: "Variable $x is unused".to_string(),
                file_path: "/src/foo.hack".to_string(),
                start_line: 10,
                start_column: 5,
                end_line: 10,
                end_column: 7,
                suggestion: None,
            }],
            scan_time_ms: 100,
            analysis_time_ms: 500,
            files_analyzed: 42,
            files_with_issues: 1,
        };

        let msg = Message::AnalyzeResult(res.clone());
        let encoded = encode_message(&msg);
        let (decoded, _) = decode_message(&encoded).unwrap();

        if let Message::AnalyzeResult(decoded_res) = decoded {
            assert_eq!(decoded_res.success, res.success);
            assert_eq!(decoded_res.issues.len(), res.issues.len());
            assert_eq!(decoded_res.issues[0].kind, res.issues[0].kind);
        } else {
            panic!("Expected AnalyzeResponse");
        }
    }

    #[test]
    fn test_roundtrip_status() {
        let msg = Message::Status(StatusRequest);
        let encoded = encode_message(&msg);
        let (decoded, _) = decode_message(&encoded).unwrap();
        assert!(matches!(decoded, Message::Status(_)));
    }

    #[test]
    fn test_roundtrip_error() {
        let err = ErrorResponse {
            code: ErrorCode::ServerBusy,
            message: "Analysis in progress".to_string(),
        };
        let msg = Message::Error(err);
        let encoded = encode_message(&msg);
        let (decoded, _) = decode_message(&encoded).unwrap();

        if let Message::Error(decoded_err) = decoded {
            assert_eq!(decoded_err.code, ErrorCode::ServerBusy);
        } else {
            panic!("Expected ErrorResponse");
        }
    }
}
