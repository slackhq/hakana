use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use std::hash::Hash;
use strum_macros::Display;

#[derive(Clone, PartialEq, Eq, Hash, Display, Debug, Serialize, Deserialize)]
pub enum TaintType {
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
    UserSecret,
    InternalSecret,
    //Custom(String),
}

impl TaintType {
    pub fn get_error_message(&self) -> String {
        match self {
            TaintType::HtmlTag => "Detected tainted HTML tags".to_string(),
            TaintType::Sql => "Detected tainted SQL".to_string(),
            TaintType::Shell => "Detected tainted shell code".to_string(),
            TaintType::FileSystem => "Detected tainted file handling".to_string(),
            TaintType::RedirectUri => "Detected a redirect URI".to_string(),
            TaintType::Unserialize => {
                "Detected tainted data passed to unserialize or similar".to_string()
            }
            //TaintType::Ldap => "Detected tainted LDAP request".to_string(),
            TaintType::Cookie => "Detected tainted cookie".to_string(),
            TaintType::CurlHeader => "Detected tainted curl header".to_string(),
            TaintType::CurlUri => "Detected tainted curl url".to_string(),
            TaintType::HtmlAttribute => "Detected tainted HTML attribute".to_string(),
            TaintType::HtmlAttributeUri => "Detected tainted HTML attribute with url".to_string(),
            TaintType::UserSecret => "Detected leak of user secret".to_string(),
            TaintType::InternalSecret => "Detected leak of internal secret".to_string(),
            //TaintType::Custom(str) => format!("Detected tainted {}", str),
        }
    }

    pub fn user_controllable_taints() -> FxHashSet<TaintType> {
        FxHashSet::from_iter([
            TaintType::HtmlTag,
            TaintType::Sql,
            TaintType::Shell,
            TaintType::FileSystem,
            TaintType::RedirectUri,
            TaintType::Unserialize,
            //TaintType::Ldap,
            TaintType::Cookie,
            TaintType::CurlHeader,
            TaintType::CurlUri,
            TaintType::HtmlAttribute,
            TaintType::HtmlAttributeUri,
        ])
    }
}

pub fn string_to_taints(str: String) -> FxHashSet<TaintType> {
    match str.as_str() {
        "input" => TaintType::user_controllable_taints(),
        "pii" | "UserSecret" => FxHashSet::from_iter([TaintType::UserSecret]),
        "sql" | "Sql" => FxHashSet::from_iter([TaintType::Sql]),
        "html" | "HtmlTag" => FxHashSet::from_iter([TaintType::HtmlTag]),
        "curl_uri" | "CurlUri" => FxHashSet::from_iter([TaintType::CurlUri]),
        "HtmlAttributeUri" => FxHashSet::from_iter([TaintType::HtmlAttributeUri]),
        "RedirectUri" => FxHashSet::from_iter([TaintType::RedirectUri]),
        _ => {
            panic!()
        }
    }
}
