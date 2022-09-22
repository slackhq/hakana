use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use std::hash::Hash;
use strum_macros::Display;

#[derive(Clone, PartialEq, Eq, Hash, Display, Debug, Serialize, Deserialize)]
pub enum SourceType {
    UriRequestHeader,
    NonUriRequestHeader,
    StoredUserData,
    UserSecret,
    SystemSecret,
}

impl SourceType {
    pub fn get_error_message(&self) -> &str {
        match self {
            SourceType::UriRequestHeader => "a URL query string",
            SourceType::NonUriRequestHeader => "a server request",
            SourceType::StoredUserData => "user-controllable storage",
            SourceType::UserSecret => "a user secret",
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
    Custom(String),
}

const PAIRS: [(SourceType, SinkType); 31] = [
    (SourceType::UriRequestHeader, SinkType::HtmlTag),
    (SourceType::UriRequestHeader, SinkType::HtmlAttribute),
    (SourceType::UriRequestHeader, SinkType::HtmlAttributeUri),
    (SourceType::UriRequestHeader, SinkType::RedirectUri),
    (SourceType::UriRequestHeader, SinkType::Cookie),
    (SourceType::UriRequestHeader, SinkType::Sql),
    (SourceType::UriRequestHeader, SinkType::Shell),
    (SourceType::UriRequestHeader, SinkType::FileSystem),
    (SourceType::UriRequestHeader, SinkType::Unserialize),
    (SourceType::UriRequestHeader, SinkType::CurlHeader),
    (SourceType::UriRequestHeader, SinkType::CurlUri),
    (SourceType::NonUriRequestHeader, SinkType::Sql),
    (SourceType::NonUriRequestHeader, SinkType::Shell),
    (SourceType::NonUriRequestHeader, SinkType::FileSystem),
    (SourceType::NonUriRequestHeader, SinkType::Unserialize),
    (SourceType::NonUriRequestHeader, SinkType::CurlHeader),
    (SourceType::NonUriRequestHeader, SinkType::CurlUri),
    (SourceType::StoredUserData, SinkType::Sql),
    (SourceType::StoredUserData, SinkType::Shell),
    (SourceType::StoredUserData, SinkType::FileSystem),
    (SourceType::StoredUserData, SinkType::Unserialize),
    (SourceType::StoredUserData, SinkType::CurlHeader),
    (SourceType::StoredUserData, SinkType::CurlUri),
    (SourceType::UserSecret, SinkType::Logging),
    (SourceType::UserSecret, SinkType::HtmlAttribute),
    (SourceType::UserSecret, SinkType::HtmlAttributeUri),
    (SourceType::UserSecret, SinkType::HtmlTag),
    (SourceType::SystemSecret, SinkType::Logging),
    (SourceType::SystemSecret, SinkType::HtmlAttribute),
    (SourceType::SystemSecret, SinkType::HtmlAttributeUri),
    (SourceType::SystemSecret, SinkType::HtmlTag),
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
        "StoredUserData" => FxHashSet::from_iter([SourceType::StoredUserData]),
        "UserSecret" => FxHashSet::from_iter([SourceType::UserSecret]),
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
        _ => {
            println!("Unrecognised annotation {}", str);
            panic!()
        }
    }
}
