use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use std::hash::Hash;
use strum_macros::Display;

#[derive(Clone, PartialEq, Eq, Hash, Display, Debug, Serialize, Deserialize)]
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

#[derive(Clone, PartialEq, Eq, Hash, Display, Debug, Serialize, Deserialize)]
pub enum SinkType {
    HtmlTag,
    Sql,
    Shell,
    FileSystem,
    RedirectUri,
    Unserialize,
    //Ldap,
    Cookie,
    CurlHeader,
    CurlUri,
    HtmlAttribute,
    HtmlAttributeUri,
    Logging,
    Output,
    Custom(String),
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

pub fn string_to_source_types(str: String) -> FxHashSet<SourceType> {
    match str.as_str() {
        "UriRequestHeader" => FxHashSet::from_iter([SourceType::UriRequestHeader]),
        "NonUriRequestHeader" => FxHashSet::from_iter([SourceType::NonUriRequestHeader]),
        "RawUserData" => FxHashSet::from_iter([SourceType::RawUserData]),
        "UserPII" => FxHashSet::from_iter([SourceType::UserPII]),
        "UserPassword" => FxHashSet::from_iter([SourceType::UserPassword]),
        "SystemSecret" => FxHashSet::from_iter([SourceType::SystemSecret]),
        _ => {
            panic!()
        }
    }
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
        "Sql" => FxHashSet::from_iter([SinkType::Sql]),
        "HtmlTag" => FxHashSet::from_iter([SinkType::HtmlTag]),
        "CurlUri" => FxHashSet::from_iter([SinkType::CurlUri]),
        "CurlHeader" => FxHashSet::from_iter([SinkType::CurlHeader]),
        "HtmlAttributeUri" => FxHashSet::from_iter([SinkType::HtmlAttributeUri]),
        "HtmlAttribute" => FxHashSet::from_iter([SinkType::HtmlAttribute]),
        "RedirectUri" => FxHashSet::from_iter([SinkType::RedirectUri]),
        "FileSystem" => FxHashSet::from_iter([SinkType::FileSystem]),
        "Logging" => FxHashSet::from_iter([SinkType::Logging]),
        "Shell" => FxHashSet::from_iter([SinkType::Shell]),
        "Unserialize" => FxHashSet::from_iter([SinkType::Unserialize]),
        "Cookie" => FxHashSet::from_iter([SinkType::Cookie]),
        "Output" => FxHashSet::from_iter([SinkType::Output]),
        _ => {
            println!("Unrecognised annotation {}", str);
            panic!()
        }
    }
}
