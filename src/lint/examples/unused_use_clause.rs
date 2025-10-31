//! Linter: Detect unused use clauses
//!
//! This is a port of HHAST's UnusedUseClauseLinter.
//! It detects use statements that import names which are never referenced in the file.

use crate::{Edit, EditSet, LintContext, LintError, Linter, Severity, SyntaxVisitor};
use parser_core_types::lexable_token::LexableToken;
use parser_core_types::syntax_by_ref::positioned_syntax::PositionedSyntax;
use parser_core_types::syntax_by_ref::positioned_token::PositionedToken;
use parser_core_types::syntax_by_ref::positioned_value::PositionedValue;
use parser_core_types::syntax_by_ref::syntax_variant_generated::*;
use parser_core_types::token_kind::TokenKind;
use rustc_hash::FxHashSet;

pub struct UnusedUseClauseLinter;

impl Linter for UnusedUseClauseLinter {
    fn name(&self) -> &'static str {
        "unused-use-clause"
    }

    fn hhast_name(&self) -> Option<&'static str> {
        Some("Facebook\\HHAST\\UnusedUseClauseLinter")
    }

    fn description(&self) -> &'static str {
        "Detects use statements that import names which are never referenced"
    }

    fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError> {
        let mut visitor = UnusedUseClauseVisitor {
            ctx,
            use_statements: Vec::new(),
            referenced_names: ReferencedNames::default(),
        };

        crate::visitor::walk(&mut visitor, ctx.root);

        // Generate errors for unused use clauses
        visitor.generate_errors()
    }

    fn supports_auto_fix(&self) -> bool {
        true
    }
}

impl UnusedUseClauseLinter {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Default)]
struct ReferencedNames {
    namespaces: FxHashSet<String>,
    types: FxHashSet<String>,
    functions: FxHashSet<String>,
}

#[derive(Debug)]
struct UseStatementInfo<'a> {
    /// The kind of use statement (namespace, type, function, or None for default)
    kind: UseKind,
    /// Individual clauses with their imported names
    clauses: Vec<UseClauseInfo>,
    /// The full use declaration node (for error reporting and deletion)
    declaration_node: &'a PositionedSyntax<'a>,
    /// Start and end offsets for deletion
    start_offset: usize,
    end_offset: usize,
    keyword_end_offset: usize,
    /// For group use statements, the prefix (e.g., "HH\Lib\" for "use namespace HH\Lib\{C, Str};")
    group_prefix: Option<String>,
}

#[derive(Debug)]
struct UseClauseInfo {
    /// The name being imported (after "as" if present, otherwise last part of qualified name)
    imported_name: String,
    /// The full path of the clause (e.g., "Bar\Baz\some_function" from group use)
    full_clause_path: String,
    /// Whether this clause has an alias (e.g., "Core as x")
    has_alias: bool,
    /// Start and end offsets for deletion
    start_offset: usize,
    end_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum UseKind {
    Namespace,
    Type,
    Function,
    Const,
    Default, // No kind specified - could be namespace or type
}

struct UnusedUseClauseVisitor<'a> {
    ctx: &'a LintContext<'a>,
    use_statements: Vec<UseStatementInfo<'a>>,
    referenced_names: ReferencedNames,
}

impl<'a> SyntaxVisitor<'a> for UnusedUseClauseVisitor<'a> {
    fn visit_namespace_use_declaration(
        &mut self,
        node: &'a NamespaceUseDeclarationChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
        // Determine the use kind
        let kind = determine_use_kind(&node.kind);

        // Extract clauses
        let clauses = extract_use_clauses(&node.clauses, self.ctx);

        if !clauses.is_empty() {
            let (start, keyword_end_offset) = self.ctx.node_range(&node.keyword);
            let (_, end) = self.ctx.node_range(&node.semicolon);

            self.use_statements.push(UseStatementInfo {
                kind,
                clauses,
                declaration_node: &node.keyword, // Store a reference node
                start_offset: start,
                keyword_end_offset: keyword_end_offset - 1, // this doesn't matter
                end_offset: end,
                group_prefix: None,
            });
        }
    }

    fn visit_namespace_group_use_declaration(
        &mut self,
        node: &'a NamespaceGroupUseDeclarationChildren<
            'a,
            PositionedToken<'a>,
            PositionedValue<'a>,
        >,
    ) {
        // Determine the use kind
        let kind = determine_use_kind(&node.kind);

        // Extract clauses (for group use, we need to handle the prefix)
        let clauses = extract_use_clauses(&node.clauses, self.ctx);

        // Extract the prefix (e.g., "HH\Lib\" from "use namespace HH\Lib\{C, Str};")
        let prefix = self.ctx.node_text(&node.prefix).trim().to_string();

        if !clauses.is_empty() {
            let (start, keyword_end_offset) = self.ctx.node_range(&node.keyword);
            let (_, end) = self.ctx.node_range(&node.semicolon);

            self.use_statements.push(UseStatementInfo {
                kind,
                clauses,
                declaration_node: &node.keyword,
                start_offset: start,
                end_offset: end,
                keyword_end_offset: keyword_end_offset - 1,
                group_prefix: Some(prefix),
            });
        }
    }

    fn visit_node(&mut self, node: &'a PositionedSyntax<'a>) {
        // Collect referenced names from various node types
        match &node.children {
            // Simple type specifier: Foo
            SyntaxVariant::SimpleTypeSpecifier(spec) => {
                if let Some(name) = extract_name_token(&spec.specifier, self.ctx) {
                    self.referenced_names.types.insert(name);
                }
            }
            // Generic type specifier: Foo<T>
            SyntaxVariant::GenericTypeSpecifier(spec) => {
                if let Some(name) = extract_name_token(&spec.class_type, self.ctx) {
                    self.referenced_names.types.insert(name);
                }
                // Also extract type arguments: in Foo<Bar, Baz>, we need to mark Bar and Baz as used
                // The argument_list contains TypeArguments which has a types field
                if let SyntaxVariant::TypeArguments(args) = &spec.argument_list.children {
                    if let SyntaxVariant::SyntaxList(types_list) = &args.types.children {
                        for type_arg in types_list.iter() {
                            if let SyntaxVariant::ListItem(item) = &type_arg.children {
                                extract_type_names(
                                    &item.item,
                                    self.ctx,
                                    &mut self.referenced_names,
                                );
                            }
                        }
                    }
                }
            }
            // Qualified name: Foo\Bar\Baz - track first part as namespace
            SyntaxVariant::QualifiedName(qn) => {
                if let SyntaxVariant::SyntaxList(parts) = &qn.parts.children {
                    if let Some(first_item) = parts.first() {
                        if let SyntaxVariant::ListItem(item) = &first_item.children {
                            if let Some(name) = extract_name_token(&item.item, self.ctx) {
                                self.referenced_names.namespaces.insert(name);
                            }
                        }
                    }
                }
            }
            // Function call: foo() or foo<Bar>()
            SyntaxVariant::FunctionCallExpression(call) => {
                if let Some(name) = extract_name_token(&call.receiver, self.ctx) {
                    self.referenced_names.functions.insert(name);
                }
                // Extract type arguments: in Cfg::get<CookiesConfig>(), we need CookiesConfig
                if let SyntaxVariant::TypeArguments(args) = &call.type_args.children {
                    if let SyntaxVariant::SyntaxList(types_list) = &args.types.children {
                        for type_arg in types_list.iter() {
                            if let SyntaxVariant::ListItem(item) = &type_arg.children {
                                extract_type_names(
                                    &item.item,
                                    self.ctx,
                                    &mut self.referenced_names,
                                );
                            }
                        }
                    }
                }
            }
            // Function pointer/reference: foo_bar<> or foo_bar<T>
            SyntaxVariant::FunctionPointerExpression(ptr) => {
                if let Some(name) = extract_name_token(&ptr.receiver, self.ctx) {
                    self.referenced_names.functions.insert(name);
                }
                // Extract type arguments: in foo_bar<SomeType>, we need SomeType
                if let SyntaxVariant::TypeArguments(args) = &ptr.type_args.children {
                    if let SyntaxVariant::SyntaxList(types_list) = &args.types.children {
                        for type_arg in types_list.iter() {
                            if let SyntaxVariant::ListItem(item) = &type_arg.children {
                                extract_type_names(
                                    &item.item,
                                    self.ctx,
                                    &mut self.referenced_names,
                                );
                            }
                        }
                    }
                }
            }
            // Scope resolution: Foo::bar
            SyntaxVariant::ScopeResolutionExpression(scope) => {
                if let Some(name) = extract_name_token(&scope.qualifier, self.ctx) {
                    self.referenced_names.types.insert(name);
                }
            }
            // Constructor call: new Foo()
            SyntaxVariant::ConstructorCall(constructor) => {
                if let Some(name) = extract_name_token(&constructor.type_, self.ctx) {
                    self.referenced_names.types.insert(name);
                }
            }
            // XHP expression: <foo:bar>...</foo:bar> or <some_namespace:foo>
            SyntaxVariant::XHPExpression(xhp) => {
                // Extract the class name from the opening tag
                if let SyntaxVariant::XHPOpen(open) = &xhp.open.children {
                    // XHP names can be:
                    // - ":foo:bar" (class foo:bar) - track "bar"
                    // - ":some_namespace:foo" where some_namespace is from "use namespace ... as some_namespace"
                    let name_text = self.ctx.node_text(&open.name).trim();

                    // Extract both the namespace part (first segment) and class part (last segment)
                    let (namespace_part, class_part) = extract_xhp_name_parts(name_text);

                    if let Some(ns) = namespace_part {
                        // If there's a namespace prefix like "some_namespace" in ":some_namespace:foo"
                        // it could be a namespace alias
                        self.referenced_names.namespaces.insert(ns);
                    }

                    if let Some(class_name) = class_part {
                        // The actual XHP class name (last segment)
                        self.referenced_names.types.insert(class_name);
                    }
                }
            }
            // Type constant: UseClass::TBar
            SyntaxVariant::TypeConstant(tc) => {
                // Extract the left side (the class name)
                if let Some(name) = extract_name_token(&tc.left_type, self.ctx) {
                    self.referenced_names.types.insert(name);
                }
                // We could also recursively process left_type if it's more complex
                extract_type_names(&tc.left_type, self.ctx, &mut self.referenced_names);
            }
            // nameof expression: nameof Foo or nameof Foo\Bar (only for classes)
            SyntaxVariant::NameofExpression(nameof_expr) => {
                // Extract the target name - nameof is only for classes/types
                if let Some(name) = extract_name_token(&nameof_expr.target, self.ctx) {
                    self.referenced_names.types.insert(name);
                } else if let SyntaxVariant::QualifiedName(qn) = &nameof_expr.target.children {
                    // Qualified name in nameof - extract first part as namespace
                    if let SyntaxVariant::SyntaxList(parts) = &qn.parts.children {
                        if let Some(first_item) = parts.first() {
                            if let SyntaxVariant::ListItem(item) = &first_item.children {
                                if let Some(name) = extract_name_token(&item.item, self.ctx) {
                                    self.referenced_names.namespaces.insert(name);
                                }
                            }
                        }
                    }
                }
            }
            // Enum class label: SomeEnum#Member
            SyntaxVariant::EnumClassLabelExpression(enum_label) => {
                // Extract the enum name (qualifier)
                if let Some(name) = extract_name_token(&enum_label.qualifier, self.ctx) {
                    self.referenced_names.types.insert(name);
                } else if let SyntaxVariant::QualifiedName(qn) = &enum_label.qualifier.children {
                    // Qualified enum name - extract first part as namespace
                    if let SyntaxVariant::SyntaxList(parts) = &qn.parts.children {
                        if let Some(first_item) = parts.first() {
                            if let SyntaxVariant::ListItem(item) = &first_item.children {
                                if let Some(name) = extract_name_token(&item.item, self.ctx) {
                                    self.referenced_names.namespaces.insert(name);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

impl<'a> UnusedUseClauseVisitor<'a> {
    fn generate_errors(&self) -> Vec<LintError> {
        let mut errors = Vec::new();

        for use_stmt in &self.use_statements {
            let mut unused_clauses = Vec::new();

            for clause in &use_stmt.clauses {
                let is_used = match use_stmt.kind {
                    UseKind::Namespace => self
                        .referenced_names
                        .namespaces
                        .contains(&clause.imported_name),
                    UseKind::Type => self.referenced_names.types.contains(&clause.imported_name),
                    UseKind::Function => self
                        .referenced_names
                        .functions
                        .contains(&clause.imported_name),
                    UseKind::Const => continue, // Skip const use statements for now
                    UseKind::Default => {
                        // Default can be namespace or type
                        self.referenced_names
                            .namespaces
                            .contains(&clause.imported_name)
                            || self.referenced_names.types.contains(&clause.imported_name)
                    }
                };

                if !is_used {
                    unused_clauses.push(clause);
                }
            }

            if !unused_clauses.is_empty() {
                let (start, end) = self.ctx.node_range(use_stmt.declaration_node);

                let message = if unused_clauses.len() == 1 {
                    format!("`{}` is not used", unused_clauses[0].imported_name)
                } else {
                    format!(
                        "{} are not used",
                        unused_clauses
                            .iter()
                            .map(|c| format!("`{}`", c.imported_name))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };

                let mut error =
                    LintError::new(Severity::Error, message, start, end, "unused-use-clause");

                // Generate auto-fix
                if self.ctx.allow_auto_fix {
                    let fix = self.generate_fix(use_stmt, &unused_clauses);
                    error = error.with_fix(fix);
                }

                errors.push(error);
            }
        }

        errors
    }

    fn generate_fix(
        &self,
        use_stmt: &UseStatementInfo<'a>,
        unused_clauses: &[&UseClauseInfo],
    ) -> EditSet {
        let mut fix = EditSet::new();
        let source_bytes = self.ctx.source.text();

        // If all clauses are unused, delete the entire statement including the newline
        if unused_clauses.len() == use_stmt.clauses.len() {
            // Find the end of the line (including newline)
            let mut end_with_newline = use_stmt.end_offset;
            if end_with_newline < source_bytes.len() && source_bytes[end_with_newline] == b'\n' {
                end_with_newline += 1;
            }
            fix.add(Edit::delete(use_stmt.start_offset, end_with_newline));
            return fix;
        }

        // Special case: group use with only one remaining clause
        // Convert to simple use statement: "use namespace HH\Lib\{C};" -> "use namespace HH\Lib\C;"
        // BUT: Don't collapse if the remaining clause has an alias, since we can't represent that
        if use_stmt.group_prefix.is_some() && unused_clauses.len() == use_stmt.clauses.len() - 1 {
            // Find the one remaining clause
            let remaining_clause = use_stmt
                .clauses
                .iter()
                .find(|c| !unused_clauses.iter().any(|u| std::ptr::eq(*u, *c)))
                .unwrap();

            // Don't collapse if the remaining clause has an alias
            // e.g., "use namespace Facebook\XHP\{Core as x};" cannot be collapsed
            if !remaining_clause.has_alias {
                // Construct the new simple use statement
                let kind_text = match use_stmt.kind {
                    UseKind::Namespace => " namespace",
                    UseKind::Type => " type",
                    UseKind::Function => " function",
                    UseKind::Const => " const",
                    UseKind::Default => "",
                };

                let prefix = use_stmt.group_prefix.as_ref().unwrap();
                let clause_path = &remaining_clause.full_clause_path;

                // Build the full qualified name by combining prefix and clause path
                let full_name = if prefix.is_empty() {
                    // No prefix, just use clause path
                    clause_path.clone()
                } else if prefix == "\\" || prefix == "" {
                    // Prefix is just "\" (root namespace), prepend to clause path
                    format!("\\{}", clause_path)
                } else if prefix.ends_with('\\') {
                    // Prefix ends with \, append clause path
                    format!("{}{}", prefix, clause_path)
                } else {
                    // Prefix doesn't end with \, add it
                    format!("{}\\{}", prefix, clause_path)
                };

                let replacement = format!("{} {};\n", kind_text, full_name);

                // Replace from start up to (but not including) the newlines we captured
                fix.add(Edit::new(
                    use_stmt.keyword_end_offset,
                    use_stmt.end_offset,
                    replacement,
                ));
                return fix;
            }
            // If has_alias, fall through to normal clause removal logic
        }

        // Group consecutive unused clauses to avoid overlapping edits
        // Sort unused clauses by start offset
        let mut sorted_unused: Vec<&UseClauseInfo> = unused_clauses.iter().copied().collect();
        sorted_unused.sort_by_key(|c| c.start_offset);

        // Group consecutive clauses together
        let mut clause_groups: Vec<Vec<&UseClauseInfo>> = Vec::new();
        let mut current_group: Vec<&UseClauseInfo> = Vec::new();

        for (i, unused) in sorted_unused.iter().enumerate() {
            current_group.push(unused);

            // Check if next clause is consecutive
            let is_last = i == sorted_unused.len() - 1;
            let next_is_consecutive = if !is_last {
                // Find the indices of current and next clauses in the original clause list
                let current_idx = use_stmt
                    .clauses
                    .iter()
                    .position(|c| std::ptr::eq(c, *unused))
                    .unwrap();
                let next_unused = sorted_unused[i + 1];
                let next_idx = use_stmt
                    .clauses
                    .iter()
                    .position(|c| std::ptr::eq(c, next_unused))
                    .unwrap();

                // Check if they are consecutive in the original list
                next_idx == current_idx + 1
            } else {
                false
            };

            if !next_is_consecutive {
                // End of consecutive group, add it and start a new group
                clause_groups.push(current_group.clone());
                current_group.clear();
            }
        }

        // Process each group of consecutive unused clauses as a single edit
        for group in clause_groups {
            let first_unused = group.first().unwrap();
            let last_unused = group.last().unwrap();

            // Look ahead for a comma after the last clause in the group
            let has_trailing_comma = if last_unused.end_offset < source_bytes.len() {
                let lookahead_end = (last_unused.end_offset + 100).min(source_bytes.len());
                let lookahead = &source_bytes[last_unused.end_offset..lookahead_end];
                if let Ok(text) = std::str::from_utf8(lookahead) {
                    // Find the next non-whitespace character
                    text.trim_start().starts_with(',')
                } else {
                    false
                }
            } else {
                false
            };

            if has_trailing_comma {
                // Find the comma position and consume it
                let mut comma_pos = None;
                for i in last_unused.end_offset..source_bytes.len() {
                    if source_bytes[i] == b',' {
                        comma_pos = Some(i);
                        break;
                    }
                    if !source_bytes[i].is_ascii_whitespace() {
                        break;
                    }
                }

                if let Some(comma_idx) = comma_pos {
                    // For trailing comma case, decide what to consume
                    let mut start = first_unused.start_offset;
                    let mut end = comma_idx + 1; // Include the comma

                    // Check what precedes the first clause in the group
                    let leading_context = if start >= 2 && source_bytes[start - 1] == b' ' {
                        // Pattern: "X Y" where X is { or ,
                        Some((source_bytes[start - 2], true)) // (char, has_space)
                    } else if start >= 1 {
                        // Pattern: "XY" where X is { or ,
                        Some((source_bytes[start - 1], false)) // (char, no_space)
                    } else {
                        None
                    };

                    // Check if followed by newline or another item
                    let has_newline_after = {
                        let mut has_nl = false;
                        for i in end..source_bytes.len() {
                            if source_bytes[i] == b'\n' {
                                has_nl = true;
                                break;
                            }
                            if !source_bytes[i].is_ascii_whitespace() {
                                break;
                            }
                        }
                        has_nl
                    };

                    if has_newline_after {
                        // Multiline case: consume up to and including newline
                        while end < source_bytes.len() {
                            if source_bytes[end] == b'\n' {
                                end += 1;
                                break;
                            }
                            end += 1;
                        }
                        // Also consume leading space if it exists
                        if let Some((_, has_space)) = leading_context {
                            if has_space {
                                start -= 1;
                            }
                        }
                    } else {
                        // Inline case
                        match leading_context {
                            Some((b'{', has_space)) => {
                                // First item after brace
                                if has_space {
                                    // "{ UnusedFirst, B}" -> should become "{B, C}" - consume space before and after comma
                                    start -= 1;
                                    if end < source_bytes.len() && source_bytes[end] == b' ' {
                                        end += 1;
                                    }
                                } else {
                                    // "{UnusedFirst, B}" -> "{B, C}" (consume ", " after item)
                                    if end < source_bytes.len() && source_bytes[end] == b' ' {
                                        end += 1;
                                    }
                                }
                            }
                            Some((b',', has_space)) => {
                                // Middle item: consume space before if present, keep space after
                                // "{A, UnusedMid, C}" -> "{A, C}"
                                if has_space {
                                    start -= 1;
                                }
                            }
                            _ => {
                                // Other cases - shouldn't normally happen
                            }
                        }
                    }

                    fix.add(Edit::delete(start, end));
                }
            } else {
                // No trailing comma - check for leading comma
                let mut has_leading_comma = false;
                let mut comma_start = first_unused.start_offset;

                if first_unused.start_offset > 0 {
                    let lookbehind_start = first_unused.start_offset.saturating_sub(100);
                    let lookbehind = &source_bytes[lookbehind_start..first_unused.start_offset];
                    if let Ok(text) = std::str::from_utf8(lookbehind) {
                        if text.trim_end().ends_with(',') {
                            has_leading_comma = true;
                            // Find the comma position
                            for i in (lookbehind_start..first_unused.start_offset).rev() {
                                if source_bytes[i] == b',' {
                                    comma_start = i;
                                    break;
                                }
                            }
                        }
                    }
                }

                if has_leading_comma {
                    // When removing with leading comma, find the end
                    let mut end = last_unused.end_offset;

                    // Check what comes after the clause
                    let has_newline_after = {
                        let mut has_nl = false;
                        for i in end..source_bytes.len() {
                            if source_bytes[i] == b'\n' {
                                has_nl = true;
                                break;
                            }
                            if !source_bytes[i].is_ascii_whitespace() {
                                break;
                            }
                        }
                        has_nl
                    };

                    if has_newline_after {
                        // Consume up to and including the newline
                        while end < source_bytes.len() {
                            if source_bytes[end] == b'\n' {
                                end += 1;
                                break;
                            }
                            end += 1;
                        }
                    } else {
                        // For inline lists, check if followed by another item
                        // If there's a space, keep it (don't consume it)
                        // The space belongs to the next item
                    }

                    fix.add(Edit::delete(comma_start, end));
                } else {
                    // No comma at all - delete from first to last clause in group
                    fix.add(Edit::delete(
                        first_unused.start_offset,
                        last_unused.end_offset,
                    ));
                }
            }
        }

        fix
    }
}

/// Determine the use kind from the kind token
fn determine_use_kind(kind_node: &PositionedSyntax) -> UseKind {
    if let Some(token) = kind_node.get_token() {
        match token.kind() {
            TokenKind::Namespace => UseKind::Namespace,
            TokenKind::Type => UseKind::Type,
            TokenKind::Function => UseKind::Function,
            TokenKind::Const => UseKind::Const,
            _ => UseKind::Default,
        }
    } else {
        UseKind::Default
    }
}

/// Extract use clauses and their imported names
fn extract_use_clauses(clauses_node: &PositionedSyntax, ctx: &LintContext) -> Vec<UseClauseInfo> {
    let mut clauses = Vec::new();

    if let SyntaxVariant::SyntaxList(list) = &clauses_node.children {
        for item in list.iter() {
            if let SyntaxVariant::ListItem(list_item) = &item.children {
                if let SyntaxVariant::NamespaceUseClause(clause) = &list_item.item.children {
                    // Get the full clause path (what's actually written in the clause)
                    let full_clause_path = ctx.node_text(&clause.name).trim().to_string();

                    // Check if this clause has an alias
                    let has_alias = !matches!(&clause.alias.children, SyntaxVariant::Missing);

                    // Get the imported name (alias or last part of qualified name)
                    let imported_name = if has_alias {
                        // Has an alias - use that
                        ctx.node_text(&clause.alias).trim().to_string()
                    } else {
                        // No alias - get last part of the name
                        extract_last_name_part(&clause.name, ctx)
                    };

                    if !imported_name.is_empty() {
                        let (start, end) = ctx.node_range(&list_item.item);
                        clauses.push(UseClauseInfo {
                            imported_name,
                            full_clause_path,
                            has_alias,
                            start_offset: start,
                            end_offset: end,
                        });
                    }
                }
            }
        }
    }

    clauses
}

/// Extract the last part of a name (for determining what was imported)
fn extract_last_name_part(name_node: &PositionedSyntax, ctx: &LintContext) -> String {
    if let Some(_token) = name_node.get_token() {
        // Simple name token
        ctx.node_text(name_node).trim().to_string()
    } else if let SyntaxVariant::QualifiedName(qn) = &name_node.children {
        // Qualified name - get last part
        if let SyntaxVariant::SyntaxList(parts) = &qn.parts.children {
            if let Some(last_item) = parts.last() {
                if let SyntaxVariant::ListItem(item) = &last_item.children {
                    return ctx.node_text(&item.item).trim().to_string();
                }
            }
        }
        String::new()
    } else {
        String::new()
    }
}

/// Extract a simple name token's text from a node
fn extract_name_token(node: &PositionedSyntax, ctx: &LintContext) -> Option<String> {
    // Check if this node is a token
    if let Some(token) = node.get_token() {
        if matches!(token.kind(), TokenKind::Name) {
            // Use ctx to get the text
            return Some(ctx.node_text(node).trim().to_string());
        }
    }
    None
}

/// Extract the namespace and class name parts from an XHP tag name
/// XHP names can be:
/// - ":foo:bar" -> namespace: Some("foo"), class: Some("bar")
/// - ":foo" -> namespace: None, class: Some("foo")
/// - ":some_namespace:button" -> namespace: Some("some_namespace"), class: Some("button")
/// Returns (namespace_part, class_part)
fn extract_xhp_name_parts(xhp_name: &str) -> (Option<String>, Option<String>) {
    // Remove leading colon if present
    let name = xhp_name.strip_prefix(':').unwrap_or(xhp_name);

    if name.is_empty() {
        return (None, None);
    }

    let segments: Vec<&str> = name.split(':').collect();

    match segments.len() {
        0 => (None, None),
        1 => {
            // Single segment like ":foo" - just a class name
            (None, Some(segments[0].to_string()))
        }
        _ => {
            // Multiple segments like ":foo:bar" or ":namespace:button"
            // First segment could be a namespace alias
            // Last segment is the class name
            (
                Some(segments[0].to_string()),
                Some(segments[segments.len() - 1].to_string()),
            )
        }
    }
}

/// Recursively extract all type names from a type specifier node
/// This handles simple types, generic types, qualified names, etc.
fn extract_type_names(node: &PositionedSyntax, ctx: &LintContext, names: &mut ReferencedNames) {
    match &node.children {
        // Simple type: Foo or qualified type: Foo\Bar
        SyntaxVariant::SimpleTypeSpecifier(spec) => {
            // Check if the specifier is a qualified name
            if let SyntaxVariant::QualifiedName(qn) = &spec.specifier.children {
                // Extract first part as namespace
                if let SyntaxVariant::SyntaxList(parts) = &qn.parts.children {
                    if let Some(first_item) = parts.first() {
                        if let SyntaxVariant::ListItem(item) = &first_item.children {
                            if let Some(name) = extract_name_token(&item.item, ctx) {
                                names.namespaces.insert(name);
                            }
                        }
                    }
                }
            } else if let Some(name) = extract_name_token(&spec.specifier, ctx) {
                // Simple name token
                names.types.insert(name);
            }
        }
        // Generic type: Foo<Bar, Baz>
        SyntaxVariant::GenericTypeSpecifier(spec) => {
            if let Some(name) = extract_name_token(&spec.class_type, ctx) {
                names.types.insert(name);
            }
            // Recursively extract type arguments
            if let SyntaxVariant::TypeArguments(args) = &spec.argument_list.children {
                if let SyntaxVariant::SyntaxList(types_list) = &args.types.children {
                    for type_arg in types_list.iter() {
                        if let SyntaxVariant::ListItem(item) = &type_arg.children {
                            extract_type_names(&item.item, ctx, names);
                        }
                    }
                }
            }
        }
        // Qualified name: Foo\Bar\Baz
        SyntaxVariant::QualifiedName(qn) => {
            if let SyntaxVariant::SyntaxList(parts) = &qn.parts.children {
                if let Some(first_item) = parts.first() {
                    if let SyntaxVariant::ListItem(item) = &first_item.children {
                        if let Some(name) = extract_name_token(&item.item, ctx) {
                            names.namespaces.insert(name);
                        }
                    }
                }
            }
        }
        // Nullable type: ?Foo
        SyntaxVariant::NullableTypeSpecifier(spec) => {
            extract_type_names(&spec.type_, ctx, names);
        }
        // Type constant: Foo::TBar
        SyntaxVariant::TypeConstant(tc) => {
            // Extract the left side (the class name)
            if let Some(name) = extract_name_token(&tc.left_type, ctx) {
                names.types.insert(name);
            }
            // Recursively process the left type in case it's complex (e.g., Foo\Bar::TBaz)
            extract_type_names(&tc.left_type, ctx, names);
        }
        // Union/intersection types, etc. - recurse into children
        _ => {
            // For other type specifiers, we could recurse more, but the main cases are covered
        }
    }
}
