use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::t_union::TUnion;
use hakana_code_info::taint::SinkType;
use oxidized::aast;

/// Discriminators belong to this element, never shared class-property storage.
/// Nonliteral values and spreads cannot establish a passive link context.
pub(super) struct AttributeContext {
    link_sink: SinkType,
}

impl AttributeContext {
    pub(super) fn new(attributes: &[aast::XhpAttribute<(), ()>]) -> Self {
        let mut rel = Some(String::new());
        let mut destination = Some(String::new());
        let mut mime_type = Some(String::new());
        for attribute in attributes {
            let aast::XhpAttribute::XhpSimple(attribute) = attribute else {
                return Self {
                    link_sink: SinkType::HtmlActiveResourceUri,
                };
            };
            let slot = match attribute.name.1.as_str() {
                "rel" => &mut rel,
                "as" => &mut destination,
                "type" => &mut mime_type,
                _ => continue,
            };
            *slot = if let aast::Expr_::String(value) = &attribute.expr.2 {
                std::str::from_utf8(value).ok().map(str::to_ascii_lowercase)
            } else {
                None
            };
        }
        Self {
            link_sink: link_sink(rel.as_deref(), destination.as_deref(), mime_type.as_deref()),
        }
    }
}

fn link_sink(rel: Option<&str>, destination: Option<&str>, mime_type: Option<&str>) -> SinkType {
    let Some(rel) = rel else {
        return SinkType::HtmlActiveResourceUri;
    };
    // A mixed rel such as "alternate stylesheet" must keep its active meaning.
    for token in rel.split_ascii_whitespace() {
        match token {
            "icon"
            | "shortcut"
            | "apple-touch-icon"
            | "apple-touch-icon-precomposed"
            | "mask-icon"
            | "canonical"
            | "alternate"
            | "author"
            | "help"
            | "license"
            | "next"
            | "prev"
            | "search"
            | "dns-prefetch"
            | "preconnect" => {}
            "preload" | "prefetch" => {
                let passive = match (destination, mime_type) {
                    (Some("image"), Some(t)) => t.is_empty() || t.starts_with("image/"),
                    (Some("audio"), Some(t)) => t.is_empty() || t.starts_with("audio/"),
                    (Some("video"), Some(t)) => t.is_empty() || t.starts_with("video/"),
                    (Some("font"), Some(t)) => t.is_empty() || t.starts_with("font/"),
                    (Some("track"), Some(t)) => t.is_empty() || t == "text/vtt",
                    _ => false,
                };
                if !passive {
                    return SinkType::HtmlActiveResourceUri;
                }
            }
            // Includes stylesheet, modulepreload, import and unknown relations.
            _ => return SinkType::HtmlActiveResourceUri,
        }
    }
    SinkType::HtmlMediaUri
}

/// Normal XHP escapes scalar values inside double quotes. Object/mixed values
/// might be UnsafeAttributeValue_DEPRECATED, which bypasses that escaping.
pub(super) fn is_escaped_value(value: &TUnion) -> bool {
    fn is_scalar(value: &TAtomic, depth: u8) -> bool {
        if depth == 0 {
            return false;
        }
        match value {
            TAtomic::TNull | TAtomic::TNothing => true,
            TAtomic::TTypeAlias {
                as_type: Some(bound),
                ..
            }
            | TAtomic::TGenericParam(hakana_code_info::t_atomic::TGenericParam {
                as_type: bound,
                ..
            }) => bound.types.iter().all(|t| is_scalar(t, depth - 1)),
            _ => value.is_some_scalar(),
        }
    }
    !value.types.is_empty() && value.types.iter().all(|t| is_scalar(t, 8))
}

pub(super) fn attribute_sinks(
    element: &str,
    name: &str,
    context: &AttributeContext,
    escaped: bool,
) -> Vec<SinkType> {
    let mut sinks = vec![SinkType::Output];
    let Some(element) = element.strip_prefix("Facebook\\XHP\\HTML\\") else {
        // A custom component's renderer does not have the native XHP guarantee.
        sinks.push(SinkType::HtmlAttribute);
        return sinks;
    };

    match (element, name) {
        ("a" | "area" | "base", "href")
        | ("form", "action")
        | ("button" | "input", "formaction") => sinks.push(SinkType::HtmlAttributeUri),
        ("script" | "iframe" | "embed", "src") | ("object", "data") => {
            sinks.push(SinkType::HtmlActiveResourceUri);
        }
        ("link", "href" | "imagesrcset") => sinks.push(context.link_sink.clone()),
        ("img" | "audio" | "video" | "source" | "track" | "input", "src")
        | ("img" | "source", "srcset")
        | ("video", "poster")
        | ("body", "background") => sinks.push(SinkType::HtmlMediaUri),
        // Attribute escaping is undone before the browser parses srcdoc as HTML.
        ("iframe", "srcdoc") => sinks.push(SinkType::HtmlTag),
        (_, "style") => sinks.push(SinkType::Css),
        (_, name) if name.starts_with("on") => sinks.push(SinkType::JavaScript),
        _ => {}
    }
    if !escaped {
        sinks.push(SinkType::HtmlAttribute);
    }
    sinks
}
