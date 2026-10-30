//! The subset of Language Server Protocol types used by Hakana.
//!
//! See https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use url::Url;

/// A zero-based position in a text document. Ordered by line, then character.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Position {
    pub line: u32,
    /// UTF-16 code unit offset within the line.
    pub character: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub uri: Url,
    pub range: Range,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiagnosticSeverity(i32);

impl DiagnosticSeverity {
    pub const ERROR: Self = Self(1);
    pub const WARNING: Self = Self(2);
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<DiagnosticSeverity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageType(i32);

impl MessageType {
    pub const ERROR: Self = Self(1);
    pub const INFO: Self = Self(3);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FileChangeType(i32);

impl FileChangeType {
    pub const CREATED: Self = Self(1);
    pub const CHANGED: Self = Self(2);
    pub const DELETED: Self = Self(3);
}

/// Bit flags selecting which file system events a watcher reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WatchKind(u8);

impl WatchKind {
    pub const DELETE: Self = Self(4);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TextDocumentSyncKind(i32);

impl TextDocumentSyncKind {
    pub const NONE: Self = Self(0);
    pub const INCREMENTAL: Self = Self(2);
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TextDocumentIdentifier {
    pub uri: Url,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TextDocumentContentChangeEvent {
    /// The replaced range, or `None` if `text` replaces the whole document.
    #[serde(default)]
    pub range: Option<Range>,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DidChangeTextDocumentParams {
    pub text_document: TextDocumentIdentifier,
    pub content_changes: Vec<TextDocumentContentChangeEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FileEvent {
    pub uri: Url,
    #[serde(rename = "type")]
    pub typ: FileChangeType,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DidChangeWatchedFilesParams {
    pub changes: Vec<FileEvent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GotoDefinitionParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentSyncOptions {
    pub open_close: bool,
    pub change: TextDocumentSyncKind,
    pub will_save: bool,
    pub will_save_wait_until: bool,
    pub save: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    pub text_document_sync: TextDocumentSyncOptions,
    pub definition_provider: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InitializeResult {
    pub capabilities: ServerCapabilities,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSystemWatcher {
    pub glob_pattern: String,
    /// Defaults to create, change and delete events when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<WatchKind>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DidChangeWatchedFilesRegistrationOptions {
    pub watchers: Vec<FileSystemWatcher>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Registration {
    pub id: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub register_options: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RegistrationParams {
    pub registrations: Vec<Registration>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PublishDiagnosticsParams {
    pub uri: Url,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LogMessageParams {
    #[serde(rename = "type")]
    pub typ: MessageType,
    pub message: String,
}
