//! Protocol message types for hakana client-server communication.

use hakana_code_info::issue::Issue;

/// Message type identifiers for the binary protocol.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    // Requests (0x01-0x7F)
    AnalyzeRequest = 0x01,
    SecurityCheckRequest = 0x02,
    GotoDefinitionRequest = 0x03,
    FindReferencesRequest = 0x04,
    FileChangedNotification = 0x05,
    GetIssuesRequest = 0x06,
    StatusRequest = 0x10,
    ShutdownRequest = 0x0F,

    // Responses (0x80-0xFE)
    AnalyzeResponse = 0x81,
    SecurityCheckResponse = 0x82,
    GotoDefinitionResponse = 0x83,
    FindReferencesResponse = 0x84,
    GetIssuesResponse = 0x85,
    StatusResponse = 0x90,
    AckResponse = 0x8F,

    // Error (0xFF)
    ErrorResponse = 0xFF,
}

impl TryFrom<u8> for MessageType {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(Self::AnalyzeRequest),
            0x02 => Ok(Self::SecurityCheckRequest),
            0x03 => Ok(Self::GotoDefinitionRequest),
            0x04 => Ok(Self::FindReferencesRequest),
            0x05 => Ok(Self::FileChangedNotification),
            0x06 => Ok(Self::GetIssuesRequest),
            0x10 => Ok(Self::StatusRequest),
            0x0F => Ok(Self::ShutdownRequest),
            0x81 => Ok(Self::AnalyzeResponse),
            0x82 => Ok(Self::SecurityCheckResponse),
            0x83 => Ok(Self::GotoDefinitionResponse),
            0x84 => Ok(Self::FindReferencesResponse),
            0x85 => Ok(Self::GetIssuesResponse),
            0x90 => Ok(Self::StatusResponse),
            0x8F => Ok(Self::AckResponse),
            0xFF => Ok(Self::ErrorResponse),
            _ => Err(value),
        }
    }
}

/// File change status for incremental analysis.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeStatus {
    Added = 0,
    Modified = 1,
    Deleted = 2,
}

impl From<hakana_orchestrator::file::FileStatus> for FileChangeStatus {
    fn from(status: hakana_orchestrator::file::FileStatus) -> Self {
        match status {
            hakana_orchestrator::file::FileStatus::Added(_, _) => Self::Added,
            hakana_orchestrator::file::FileStatus::Modified(_, _) => Self::Modified,
            hakana_orchestrator::file::FileStatus::Deleted => Self::Deleted,
            hakana_orchestrator::file::FileStatus::DeletedDir => Self::Deleted,
            hakana_orchestrator::file::FileStatus::Unchanged(_, _) => Self::Modified,
        }
    }
}

/// A file change notification.
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub status: FileChangeStatus,
}

/// Request to analyze the codebase.
#[derive(Debug, Clone, Default)]
pub struct AnalyzeRequest {
    /// Filter analysis to specific path prefix.
    pub filter: Option<String>,
    /// Find unused expressions (data flow analysis).
    pub find_unused_expressions: bool,
    /// Find unused definitions (dead code).
    pub find_unused_definitions: bool,
    /// Incremental file changes since last analysis.
    pub file_changes: Vec<FileChange>,
    /// Force a full rescan of all files.
    pub full_rescan: bool,
    /// Allowed issue kinds (None = all issues).
    pub allowed_issues: Option<Vec<String>>,
}

/// Response from an analysis request.
#[derive(Debug, Clone)]
pub struct AnalyzeResponse {
    /// Whether analysis completed successfully.
    pub success: bool,
    /// Issues found during analysis.
    pub issues: Vec<ProtocolIssue>,
    /// Time spent scanning files (milliseconds).
    pub scan_time_ms: u64,
    /// Time spent analyzing (milliseconds).
    pub analysis_time_ms: u64,
    /// Number of files analyzed.
    pub files_analyzed: u32,
    /// Number of files with issues.
    pub files_with_issues: u32,
}

/// A simplified issue representation for the protocol.
#[derive(Debug, Clone)]
pub struct ProtocolIssue {
    pub kind: String,
    pub description: String,
    pub file_path: String,
    pub start_line: u32,
    pub start_column: u16,
    pub end_line: u32,
    pub end_column: u16,
    /// Optional fix suggestion.
    pub suggestion: Option<String>,
}

impl ProtocolIssue {
    pub fn from_issue(issue: &Issue, file_path: &str) -> Self {
        Self {
            kind: issue.kind.to_string(),
            description: issue.description.clone(),
            file_path: file_path.to_string(),
            start_line: issue.pos.start_line,
            start_column: issue.pos.start_column,
            end_line: issue.pos.end_line,
            end_column: issue.pos.end_column,
            suggestion: None, // TODO: extract from issue if available
        }
    }
}

/// Request for security/taint analysis.
#[derive(Debug, Clone, Default)]
pub struct SecurityCheckRequest {
    /// Maximum depth for taint tracking.
    pub max_depth: Option<u32>,
    /// Filter to specific path prefix.
    pub filter: Option<String>,
}

/// Response from security check.
#[derive(Debug, Clone)]
pub struct SecurityCheckResponse {
    pub success: bool,
    pub issues: Vec<ProtocolIssue>,
    pub taint_flows_found: u32,
    pub analysis_time_ms: u64,
}

/// Request for goto-definition.
#[derive(Debug, Clone)]
pub struct GotoDefinitionRequest {
    pub file_path: String,
    pub line: u32,
    pub column: u32,
}

/// Response with definition location.
#[derive(Debug, Clone)]
pub struct GotoDefinitionResponse {
    pub found: bool,
    pub file_path: Option<String>,
    pub start_line: Option<u32>,
    pub start_column: Option<u16>,
    pub end_line: Option<u32>,
    pub end_column: Option<u16>,
}

/// Request for find-references.
#[derive(Debug, Clone)]
pub struct FindReferencesRequest {
    pub file_path: String,
    pub line: u32,
    pub column: u32,
}

/// A single reference location.
#[derive(Debug, Clone)]
pub struct ReferenceLocation {
    pub file_path: String,
    pub line: u32,
    pub column: u16,
}

/// Response with reference locations.
#[derive(Debug, Clone)]
pub struct FindReferencesResponse {
    pub references: Vec<ReferenceLocation>,
}

/// Request for current issues (used by CLI client).
#[derive(Debug, Clone, Default)]
pub struct GetIssuesRequest {
    /// Filter to specific path prefix.
    pub filter: Option<String>,
    /// Find unused expressions (data flow analysis).
    pub find_unused_expressions: bool,
    /// Find unused definitions (dead code).
    pub find_unused_definitions: bool,
}

/// Response with current issues.
#[derive(Debug, Clone)]
pub struct GetIssuesResponse {
    /// Whether analysis is complete (false = still in progress).
    pub analysis_complete: bool,
    /// Issues found during analysis (may be partial if analysis_complete is false).
    pub issues: Vec<ProtocolIssue>,
    /// Number of files analyzed so far.
    pub files_analyzed: u32,
    /// Total number of files to analyze (0 if unknown).
    pub total_files: u32,
    /// Current analysis phase description.
    pub phase: String,
    /// Progress percentage (0-100).
    pub progress_percent: u8,
}

/// Request for server status.
#[derive(Debug, Clone, Copy)]
pub struct StatusRequest;

/// Server status response.
#[derive(Debug, Clone)]
pub struct StatusResponse {
    /// Server is ready to accept requests.
    pub ready: bool,
    /// Number of files in codebase.
    pub files_count: u32,
    /// Number of symbols indexed.
    pub symbols_count: u32,
    /// Server uptime in seconds.
    pub uptime_secs: u64,
    /// Whether an analysis is currently running.
    pub analysis_in_progress: bool,
    /// Number of pending requests in queue.
    pub pending_requests: u32,
    /// Project root path.
    pub project_root: String,
}

/// Shutdown request (no payload).
#[derive(Debug, Clone, Copy)]
pub struct ShutdownRequest;

/// Acknowledgment response (for notifications and shutdown).
#[derive(Debug, Clone, Copy)]
pub struct AckResponse;

/// Error response.
#[derive(Debug, Clone)]
pub struct ErrorResponse {
    pub code: ErrorCode,
    pub message: String,
}

/// Error codes for the protocol.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Unknown or internal error.
    Unknown = 0,
    /// Invalid request format.
    InvalidRequest = 1,
    /// Unsupported message type.
    UnsupportedMessage = 2,
    /// Server is busy (analysis in progress).
    ServerBusy = 3,
    /// Configuration error.
    ConfigError = 4,
    /// File not found.
    FileNotFound = 5,
    /// Analysis failed.
    AnalysisFailed = 6,
    /// Server is shutting down.
    ShuttingDown = 7,
}

impl TryFrom<u32> for ErrorCode {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::InvalidRequest),
            2 => Ok(Self::UnsupportedMessage),
            3 => Ok(Self::ServerBusy),
            4 => Ok(Self::ConfigError),
            5 => Ok(Self::FileNotFound),
            6 => Ok(Self::AnalysisFailed),
            7 => Ok(Self::ShuttingDown),
            _ => Err(value),
        }
    }
}

/// Envelope for all protocol messages.
#[derive(Debug, Clone)]
pub enum Message {
    // Requests
    Analyze(AnalyzeRequest),
    SecurityCheck(SecurityCheckRequest),
    GotoDefinition(GotoDefinitionRequest),
    FindReferences(FindReferencesRequest),
    FileChanged(Vec<FileChange>),
    GetIssues(GetIssuesRequest),
    Status(StatusRequest),
    Shutdown(ShutdownRequest),

    // Responses
    AnalyzeResult(AnalyzeResponse),
    SecurityCheckResult(SecurityCheckResponse),
    GotoDefinitionResult(GotoDefinitionResponse),
    FindReferencesResult(FindReferencesResponse),
    GetIssuesResult(GetIssuesResponse),
    StatusResult(StatusResponse),
    Ack(AckResponse),
    Error(ErrorResponse),
}

impl Message {
    /// Get the message type identifier.
    pub fn message_type(&self) -> MessageType {
        match self {
            Self::Analyze(_) => MessageType::AnalyzeRequest,
            Self::SecurityCheck(_) => MessageType::SecurityCheckRequest,
            Self::GotoDefinition(_) => MessageType::GotoDefinitionRequest,
            Self::FindReferences(_) => MessageType::FindReferencesRequest,
            Self::FileChanged(_) => MessageType::FileChangedNotification,
            Self::GetIssues(_) => MessageType::GetIssuesRequest,
            Self::Status(_) => MessageType::StatusRequest,
            Self::Shutdown(_) => MessageType::ShutdownRequest,
            Self::AnalyzeResult(_) => MessageType::AnalyzeResponse,
            Self::SecurityCheckResult(_) => MessageType::SecurityCheckResponse,
            Self::GotoDefinitionResult(_) => MessageType::GotoDefinitionResponse,
            Self::FindReferencesResult(_) => MessageType::FindReferencesResponse,
            Self::GetIssuesResult(_) => MessageType::GetIssuesResponse,
            Self::StatusResult(_) => MessageType::StatusResponse,
            Self::Ack(_) => MessageType::AckResponse,
            Self::Error(_) => MessageType::ErrorResponse,
        }
    }

    /// Check if this is a request message.
    pub fn is_request(&self) -> bool {
        matches!(
            self,
            Self::Analyze(_)
                | Self::SecurityCheck(_)
                | Self::GotoDefinition(_)
                | Self::FindReferences(_)
                | Self::FileChanged(_)
                | Self::GetIssues(_)
                | Self::Status(_)
                | Self::Shutdown(_)
        )
    }

    /// Check if this is a response message.
    pub fn is_response(&self) -> bool {
        !self.is_request()
    }
}
