#[derive(Debug, Clone, PartialEq)]
pub enum ClientType {
    /// Language Server Protocol client (IDE integrations)
    Lsp,
    /// Model Context Protocol client (AI tools)
    Mcp,
    /// Command-line interface client
    Cli,
    /// Unknown client type
    Unknown,
}

impl ClientType {
    pub fn from_name(name: &str) -> Self {
        match name {
            "hakana-lsp" => Self::Lsp,
            "hakana-mcp" => Self::Mcp,
            "hakana-cli" => Self::Cli,
            _ => Self::Unknown,
        }
    }

    pub fn should_receive_diagnostics(&self) -> bool {
        matches!(self, Self::Lsp)
    }

    pub fn should_receive_file_changes(&self) -> bool {
        matches!(self, Self::Lsp | Self::Mcp)
    }
}