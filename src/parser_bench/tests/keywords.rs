//! Checks the native lexer's keyword table against HHVM's.
//!
//! HHVM's `token_kind.rs` is generated from its syntax schema. This test
//! reads the keyword list out of that file, so an HHVM upgrade that adds,
//! removes or re-classifies a keyword fails here with the exact difference.

use std::collections::BTreeMap;

use hakana_parser::token::{T, keyword};

const HHVM_TOKEN_KIND: &str =
    include_str!("../../../third-party/hhvm/hphp/hack/src/parser/token_kind.rs");

/// Keyword text -> whether HHVM treats it as reserved.
fn hhvm_keywords() -> BTreeMap<String, bool> {
    let start = HHVM_TOKEN_KIND
        .find("pub fn from_string")
        .expect("TokenKind::from_string not found");
    let body = &HHVM_TOKEN_KIND[start..];
    let end = body
        .find("\n    }\n")
        .expect("end of from_string not found");
    let mut out = BTreeMap::new();
    for line in body[..end].lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("b\"") else {
            continue;
        };
        let Some((text, rest)) = rest.split_once('"') else {
            continue;
        };
        // Punctuation is handled by the lexer directly.
        if !text.bytes().all(|b| b.is_ascii_alphabetic() || b == b'_') {
            continue;
        }
        out.insert(text.to_string(), !rest.contains("if !only_reserved"));
    }
    out
}

#[test]
fn keyword_table_matches_hhvm() {
    let hhvm = hhvm_keywords();
    assert!(hhvm.len() > 100, "failed to read HHVM's keyword list");

    let mut problems = vec![];
    for (text, reserved) in &hhvm {
        match keyword(text.as_bytes()) {
            None => problems.push(format!("`{text}` is a keyword in HHVM but not here")),
            Some(kind) => {
                // true/false/null are literals, not names, in both lexers.
                let is_literal = matches!(kind, T::True | T::False | T::Null);
                if !is_literal && kind.is_reserved() != *reserved {
                    problems.push(format!(
                        "`{text}`: HHVM reserved = {reserved}, here = {}",
                        kind.is_reserved()
                    ));
                }
            }
        }
    }

    // And nothing here that HHVM does not have.
    for candidate in include_str!("../../parser/token.rs")
        .lines()
        .filter_map(|l| l.trim().strip_prefix("b\""))
        .filter_map(|l| l.split_once('"'))
        .map(|(text, _)| text)
    {
        if keyword(candidate.as_bytes()).is_some() && !hhvm.contains_key(candidate) {
            problems.push(format!("`{candidate}` is a keyword here but not in HHVM"));
        }
    }

    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
