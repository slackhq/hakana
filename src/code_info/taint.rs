use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use std::{hash::Hash, str::FromStr};
use strum_macros::{Display, EnumString};

#[derive(Clone, PartialEq, Eq, Hash, Display, Debug, Serialize, Deserialize, EnumString)]
pub enum SourceType {
    UriRequestHeader,
    NonUriRequestHeader,
    RawUserData,
    UserPII,
    UserPassword,
    SystemSecret,
}

impl SourceType {
    pub fn get_error_message(&self) -> &str {
        match self {
            SourceType::UriRequestHeader => "a URL query string",
            SourceType::NonUriRequestHeader => "a server request",
            SourceType::RawUserData => "raw user-controllable data",
            SourceType::UserPassword => "a user secret",
            SourceType::UserPII => "PII user data",
            SourceType::SystemSecret => "a system secret",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Display, Debug, Serialize, Deserialize, EnumString)]
pub enum SinkType {
    HtmlTag,
    Sql,
    Shell,
    FileSystem,
    RedirectUri,
    Unserialize,
    Cookie,
    CurlHeader,
    CurlUri,
    HtmlAttribute,
    HtmlAttributeUri,
    Logging,
    Output,
    Custom(String),
}

impl Default for SinkType {
    fn default() -> Self {
        SinkType::HtmlTag
    }
}

const PAIRS: [(SourceType, SinkType); 32] = [
    // All the places we don't want GET data to go
    (SourceType::UriRequestHeader, SinkType::Sql),
    (SourceType::UriRequestHeader, SinkType::Shell),
    (SourceType::UriRequestHeader, SinkType::FileSystem),
    (SourceType::UriRequestHeader, SinkType::Unserialize),
    (SourceType::UriRequestHeader, SinkType::CurlHeader),
    (SourceType::UriRequestHeader, SinkType::CurlUri),
    (SourceType::UriRequestHeader, SinkType::HtmlAttribute),
    (SourceType::UriRequestHeader, SinkType::HtmlAttributeUri),
    (SourceType::UriRequestHeader, SinkType::HtmlTag),
    (SourceType::UriRequestHeader, SinkType::RedirectUri),
    (SourceType::UriRequestHeader, SinkType::Cookie),
    // We don't want unescaped user data in any of those places either
    // Except we allow it in cookies
    (SourceType::RawUserData, SinkType::Sql),
    (SourceType::RawUserData, SinkType::Shell),
    (SourceType::RawUserData, SinkType::FileSystem),
    (SourceType::RawUserData, SinkType::Unserialize),
    (SourceType::RawUserData, SinkType::CurlHeader),
    (SourceType::RawUserData, SinkType::CurlUri),
    (SourceType::RawUserData, SinkType::HtmlAttribute),
    (SourceType::RawUserData, SinkType::HtmlAttributeUri),
    (SourceType::RawUserData, SinkType::HtmlTag),
    (SourceType::RawUserData, SinkType::RedirectUri),
    // All the places we don't want POST data to go
    // For example we don't care about XSS in POST data
    (SourceType::NonUriRequestHeader, SinkType::Sql),
    (SourceType::NonUriRequestHeader, SinkType::Shell),
    (SourceType::NonUriRequestHeader, SinkType::FileSystem),
    (SourceType::NonUriRequestHeader, SinkType::Unserialize),
    (SourceType::NonUriRequestHeader, SinkType::CurlHeader),
    (SourceType::NonUriRequestHeader, SinkType::CurlUri),
    // We don't want user PII to appear in logs, but it's
    // ok for it to appear everywhere else.
    (SourceType::UserPII, SinkType::Logging),
    // User passwords shouldn't appear in any user output or logs
    (SourceType::UserPassword, SinkType::Logging),
    (SourceType::UserPassword, SinkType::Output),
    // System secrets have the same prohibitions
    (SourceType::SystemSecret, SinkType::Logging),
    (SourceType::SystemSecret, SinkType::Output),
];

pub fn get_sinks_for_sources(source: &SourceType) -> FxHashSet<SinkType> {
    PAIRS
        .into_iter()
        .filter(|p| &p.0 == source)
        .map(|p| p.1)
        .collect()
}

impl SinkType {
    pub fn get_error_message(&self) -> String {
        match self {
            SinkType::HtmlTag => "an HTML tag".to_string(),
            SinkType::Sql => "a SQL query".to_string(),
            SinkType::Shell => "a shell command".to_string(),
            SinkType::FileSystem => "a filesystem call".to_string(),
            SinkType::RedirectUri => "a redirect URI".to_string(),
            SinkType::Unserialize => "to unserialize or similar".to_string(),
            //TaintType::Ldap => "Detected tainted LDAP request".to_string(),
            SinkType::Cookie => "a cookie".to_string(),
            SinkType::CurlHeader => "a curl header".to_string(),
            SinkType::CurlUri => "a curl url".to_string(),
            SinkType::HtmlAttribute => "an HTML attribute".to_string(),
            SinkType::HtmlAttributeUri => "an HTML attribute with url".to_string(),
            SinkType::Logging => "a logging method".to_string(),
            SinkType::Output => "generic output".to_string(),
            SinkType::Custom(str) => format!("Detected data passed to {}", str),
        }
    }

    pub fn user_controllable_taints() -> FxHashSet<SinkType> {
        FxHashSet::from_iter([
            SinkType::HtmlTag,
            SinkType::Sql,
            SinkType::Shell,
            SinkType::FileSystem,
            SinkType::RedirectUri,
            SinkType::Unserialize,
            //TaintType::Ldap,
            SinkType::Cookie,
            SinkType::CurlHeader,
            SinkType::CurlUri,
            SinkType::HtmlAttribute,
            SinkType::HtmlAttributeUri,
        ])
    }
}

pub fn string_to_source_types(str: String) -> Option<SourceType> {
    SourceType::from_str(&str).ok()
}

pub fn string_to_sink_types(str: String) -> FxHashSet<SinkType> {
    match str.as_str() {
        "*" => FxHashSet::from_iter([
            SinkType::Sql,
            SinkType::HtmlTag,
            SinkType::HtmlAttribute,
            SinkType::HtmlAttributeUri,
            SinkType::CurlHeader,
            SinkType::CurlUri,
            SinkType::FileSystem,
            SinkType::RedirectUri,
            SinkType::Shell,
            SinkType::Unserialize,
            SinkType::Cookie,
        ]),
        str => {
            if let Ok(sink_type) = SinkType::from_str(&str) {
                FxHashSet::from_iter([sink_type])
            } else if str.starts_with("Custom:") {
                FxHashSet::from_iter([SinkType::Custom(str.get(7..).unwrap().to_string())])
            } else {
                FxHashSet::default()
            }
        }
    }
}
