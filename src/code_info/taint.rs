use serde::{Deserialize, Serialize};
use std::{hash::Hash, str::FromStr};
use strum_macros::{Display, EnumString};

#[derive(Clone, PartialEq, Eq, Hash, Display, Debug, Serialize, Deserialize, EnumString)]
pub enum SourceType {
    UriRequestHeader,
    NonUriRequestHeader,
    RawUserData,
    UserData,
    UserEmail,
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
            SourceType::UserData => "user-generated content",
            SourceType::UserEmail => "a user email string",
            SourceType::UserPII => "PII user data",
            SourceType::UserPassword => "a user secret",
            SourceType::SystemSecret => "a system secret",
        }
    }
}

#[derive(
    Clone, PartialEq, Eq, Hash, Display, Debug, Serialize, Deserialize, EnumString, Default,
)]
pub enum SinkType {
    #[default]
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
    JavaScript,
    Css,
    ResponseHeader,
    Logging,
    Output,
    UnauthorizedDataFetchKey,
    Custom(String),
    /// A URL whose contents may execute as script, a document, or a stylesheet.
    HtmlActiveResourceUri,
    /// A passive browser resource, not an HTML/JavaScript injection sink.
    HtmlMediaUri,
}

/// Injection policy is independent of the transport carrying untrusted input.
pub fn get_sinks_for_sources(source: &SourceType) -> Vec<SinkType> {
    match source {
        SourceType::UriRequestHeader | SourceType::NonUriRequestHeader => {
            let mut sinks = SinkType::user_controllable_taints();
            sinks.push(SinkType::UnauthorizedDataFetchKey);
            sinks
        }
        SourceType::RawUserData => SinkType::user_controllable_taints()
            .into_iter()
            .filter(|sink| *sink != SinkType::Cookie)
            .collect(),
        SourceType::UserData | SourceType::UserEmail | SourceType::UserPII => {
            vec![SinkType::Logging]
        }
        SourceType::UserPassword | SourceType::SystemSecret => {
            vec![SinkType::Logging, SinkType::Output]
        }
    }
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
            SinkType::HtmlActiveResourceUri => "an executable HTML resource URL".to_string(),
            SinkType::HtmlMediaUri => "a passive HTML media URL".to_string(),
            SinkType::JavaScript => "JavaScript code".to_string(),
            SinkType::Css => "CSS code".to_string(),
            SinkType::ResponseHeader => "an HTTP response header".to_string(),
            SinkType::Logging => "a logging method".to_string(),
            SinkType::Output => "generic output".to_string(),
            SinkType::UnauthorizedDataFetchKey => "unauthorized fetch key for data".to_string(),
            SinkType::Custom(str) => format!("Detected data passed to {}", str),
        }
    }

    pub fn user_controllable_taints() -> Vec<SinkType> {
        vec![
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
            SinkType::HtmlActiveResourceUri,
            // HtmlMediaUri is deliberately excluded: user-selected media URLs
            // do not execute HTML/JavaScript. Output still tracks secret leaks.
            SinkType::JavaScript,
            SinkType::Css,
            SinkType::ResponseHeader,
        ]
    }
}

pub fn string_to_source_types(str: String) -> Option<SourceType> {
    SourceType::from_str(&str).ok()
}

/// Values supplied by the HTTP client, as opposed to server configuration.
pub fn is_request_server_key(key: &str) -> bool {
    key.starts_with("HTTP_")
        || matches!(
            key,
            "CONTENT_TYPE"
                | "CONTENT_LENGTH"
                | "QUERY_STRING"
                | "REQUEST_URI"
                | "PATH_INFO"
                | "ORIG_PATH_INFO"
                | "PHP_SELF"
                | "REDIRECT_URL"
                | "SERVER_NAME"
                | "PHP_AUTH_USER"
                | "PHP_AUTH_PW"
                | "PHP_AUTH_DIGEST"
                | "REMOTE_USER"
                | "REDIRECT_REMOTE_USER"
        )
}

pub fn string_to_sink_types(str: String) -> Vec<SinkType> {
    match str.as_str() {
        "*" => SinkType::user_controllable_taints(),
        str => {
            if let Ok(sink_type) = SinkType::from_str(str) {
                vec![sink_type]
            } else if str.starts_with("Custom:") {
                vec![SinkType::Custom(str.get(7..).unwrap().to_string())]
            } else {
                vec![]
            }
        }
    }
}
