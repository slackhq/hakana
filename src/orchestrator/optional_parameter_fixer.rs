use hakana_code_info::{analysis_result::Replacement, functionlike_info::FunctionLikeInfo};
use hakana_str::Interner;

/// Apply a signature edit as one replacement so adjacent removed parameters
/// cannot leave overlapping edits or stray commas.
pub(crate) fn plan(
    info: &FunctionLikeInfo,
    interner: &Interner,
    source: &str,
    selected: &[usize],
) -> Option<Vec<((u32, u32), Replacement)>> {
    let name = info.name_location?;
    let end = info.def_location.end_offset as usize;
    let first_statement = info
        .first_statement_offset
        .map(|p| p as usize)
        .unwrap_or(end - 1);
    if end > source.len() || first_statement >= end {
        return None;
    }
    let tokens = punctuation(source, name.end_offset as usize, first_statement + 1)?;
    let mut generics = 0;
    let open = tokens.iter().find_map(|&(offset, ch)| {
        match ch {
            b'<' => generics += 1,
            b'>' => generics -= 1,
            b'(' if generics == 0 => return Some(offset),
            _ => {}
        }
        None
    })?;
    let mut depth = 0;
    let close = tokens
        .iter()
        .filter(|(offset, _)| *offset >= open)
        .find_map(|&(offset, ch)| {
            match ch {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(offset);
                    }
                }
                _ => {}
            }
            None
        })?;
    let has_body = source.as_bytes().get(end - 1) == Some(&b'}');
    let body_open = if has_body {
        tokens
            .iter()
            .filter(|(offset, ch)| *offset > close && *offset <= first_statement && *ch == b'{')
            .map(|(offset, _)| *offset)
            .last()?
    } else {
        end - 1
    };
    let header = &source[close + 1..body_open];
    let mut properties = vec![];
    let mut locals = vec![];
    for &index in selected {
        let param = info.params.get(index)?;
        let default = param.default_value_location?;
        let variable = interner.lookup(&param.name.0);
        if header.contains(variable) {
            // Removing a capability-dependent parameter also changes coeffects.
            return None;
        }
        let value = source.get(default.start_offset as usize..default.end_offset as usize)?;
        if value.contains("__LINE__") || value.contains("<<<") {
            return None;
        }
        let start = if index == 0 {
            open + 1
        } else {
            let previous = &info.params[index - 1];
            let previous_end = previous
                .default_value_location
                .map(|p| p.end_offset)
                .unwrap_or(previous.name_location.end_offset)
                as usize;
            tokens
                .iter()
                .find(|(offset, ch)| *offset >= previous_end && *offset < close && *ch == b',')?
                .0
                + 1
        };
        let type_start = param
            .signature_type_location
            .unwrap_or(param.name_location)
            .start_offset as usize;
        let comments = leading_comments(source.get(start..type_start)?);
        locals.push(format!("{comments}{variable} = {value};"));
        if param.promoted_property {
            if !has_body || !param.attributes.is_empty() {
                return None;
            }
            let ty = param.signature_type_location?;
            let mut prefix = source[start..ty.start_offset as usize].trim_start();
            while prefix.starts_with("/*") || prefix.starts_with("//") {
                let end = if prefix.starts_with("/*") {
                    prefix.find("*/")? + 2
                } else {
                    prefix.find('\n')?
                };
                prefix = prefix[end..].trim_start();
            }
            if !prefix
                .split_whitespace()
                .all(|word| matches!(word, "public" | "protected" | "private" | "readonly"))
            {
                return None;
            }
            if !prefix
                .split_whitespace()
                .any(|word| matches!(word, "public" | "protected" | "private"))
            {
                return None;
            }
            properties.push(format!(
                "{} {} {variable};",
                prefix.trim(),
                &source[ty.start_offset as usize..ty.end_offset as usize]
            ));
            locals.push(format!(
                "$this->{} = {variable};",
                variable.trim_start_matches('$')
            ));
        }
    }

    let mut segments = vec![];
    let mut start = open + 1;
    let mut trailing_comma = false;
    for (index, param) in info.params.iter().enumerate() {
        let param_end = param
            .default_value_location
            .map(|p| p.end_offset)
            .unwrap_or(param.name_location.end_offset) as usize;
        let separator = tokens
            .iter()
            .find(|(offset, ch)| *offset >= param_end && *offset < close && *ch == b',')
            .map(|(offset, _)| *offset);
        let segment_end = separator.unwrap_or(close);
        if !selected.contains(&index) {
            segments.push(&source[start..segment_end]);
        }
        start = separator.map(|offset| offset + 1).unwrap_or(close);
        trailing_comma = separator.is_some();
    }
    let mut parameters = segments.join(",");
    if !parameters.starts_with('\n') {
        parameters = parameters.trim_start_matches([' ', '\t']).to_string();
    }
    if !segments.is_empty() && trailing_comma {
        parameters.push(',');
        parameters.push_str(&source[start..close]);
    }
    let mut replacements = vec![(
        ((open + 1) as u32, close as u32),
        Replacement::Substitute(parameters),
    )];
    if !has_body {
        return Some(replacements);
    }
    let declaration_indent = source[..info.def_location.start_offset as usize]
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>();
    let body_indent = format!("{declaration_indent}\t");
    let body_suffix = &source[body_open + 1..end];
    let locals = locals
        .iter()
        .map(|local| local.replace('\n', &format!("\n{body_indent}")))
        .collect::<Vec<_>>();
    let empty_body = body_suffix.trim() == "}";
    let insertion = if empty_body {
        format!(
            "\n{body_indent}{}\n{declaration_indent}",
            locals.join(&format!("\n{body_indent}"))
        )
    } else if body_suffix.starts_with('\n') {
        format!(
            "\n{}{}",
            body_indent,
            locals.join(&format!("\n{body_indent}"))
        )
    } else if locals.iter().any(|local| local.contains('\n')) {
        format!(
            "\n{}{}\n{}",
            body_indent,
            locals.join(&format!("\n{body_indent}")),
            body_indent
        )
    } else {
        format!(
            " {}{}",
            locals.join(" "),
            if body_suffix.starts_with(char::is_whitespace) {
                ""
            } else {
                " "
            }
        )
    };
    replacements.push((
        (
            (body_open + 1) as u32,
            if empty_body {
                (end - 1) as u32
            } else {
                (body_open + 1) as u32
            },
        ),
        Replacement::Substitute(insertion),
    ));
    if !properties.is_empty() {
        // A constructor's scanned comment prefix can include the class docblock.
        // Never place its properties outside the enclosing class.
        let offset = if punctuation(
            source,
            info.meta_start.start_offset as usize,
            info.def_location.start_offset as usize,
        )?
        .iter()
        .any(|(_, ch)| *ch == b'{')
        {
            info.def_location.start_offset
        } else {
            info.meta_start.start_offset
        };
        replacements.push((
            (offset, offset),
            Replacement::Substitute(format!(
                "{}\n\n{declaration_indent}",
                properties.join(&format!("\n{declaration_indent}"))
            )),
        ));
    }
    Some(replacements)
}

fn leading_comments(mut prefix: &str) -> String {
    let mut comments = String::new();
    loop {
        prefix = prefix.trim_start();
        let end = if prefix.starts_with("/*") {
            prefix.find("*/").map(|end| end + 2)
        } else if prefix.starts_with("//") {
            Some(prefix.find('\n').unwrap_or(prefix.len()))
        } else {
            None
        };
        let Some(end) = end else {
            break;
        };
        if prefix[..end].contains("HAKANA_FIXME[UnusedParameter]")
            || prefix[..end].contains("HAKANA_IGNORE[UnusedParameter]")
        {
            // This suppression belonged to the removed parameter, not its local.
            prefix = &prefix[end..];
            continue;
        }
        comments.push_str(
            &prefix[..end]
                .lines()
                .map(str::trim)
                .collect::<Vec<_>>()
                .join("\n"),
        );
        comments.push('\n');
        prefix = &prefix[end..];
    }
    comments
}

/// Punctuation outside comments and quoted strings, retaining byte positions.
/// Unsupported heredocs are left for manual review.
fn punctuation(source: &str, start: usize, end: usize) -> Option<Vec<(usize, u8)>> {
    let bytes = source.as_bytes();
    let mut tokens = vec![];
    let mut i = start;
    while i < end {
        match bytes[i] {
            b'\'' | b'"' | b'`' => {
                let quote = bytes[i];
                i += 1;
                while i < end && bytes[i] != quote {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < end && &bytes[i..i + 2] != b"*/" {
                    i += 1;
                }
                i += 2;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < end && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'<' if bytes.get(i..i + 3) == Some(b"<<<")
                && !source[i..].split('\n').next()?.contains(">>") =>
            {
                return None;
            }
            ch => {
                if b"<>(){},;".contains(&ch) {
                    tokens.push((i, ch));
                }
                i += 1;
            }
        }
    }
    Some(tokens)
}
