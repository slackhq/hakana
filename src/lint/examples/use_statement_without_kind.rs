//! Linter: Detect use statements without explicit kind
//!
//! This is a port of HHAST's UseStatementWithoutKindLinter.
//! It ensures that namespace use declarations have an explicit kind keyword
//! (type, namespace, function, const).

use crate::{Edit, EditSet, LintContext, LintError, Linter, Severity, SyntaxVisitor};
use parser_core_types::lexable_token::LexableToken;
use parser_core_types::syntax_by_ref::positioned_syntax::PositionedSyntax;
use parser_core_types::syntax_by_ref::syntax_variant_generated::SyntaxVariant;
use parser_core_types::syntax_trait::SyntaxTrait;
use parser_core_types::token_kind::TokenKind;
use std::collections::HashMap;

pub struct UseStatementWithoutKindLinter;

impl Linter for UseStatementWithoutKindLinter {
    fn name(&self) -> &'static str {
        "use-statement-without-kind"
    }

    fn hhast_name(&self) -> Option<&'static str> {
        Some("Facebook\\HHAST\\UseStatementWithoutKindLinter")
    }

    fn description(&self) -> &'static str {
        "Ensures use statements have explicit kind (type, namespace, function, const)"
    }

    fn lint<'a>(&self, ctx: &LintContext<'a>) -> Vec<LintError> {
        // Two-pass analysis:
        // 1. Collect all use statements without kind
        // 2. Analyze how each imported name is used in the file

        let mut collector = UseStatementCollector {
            ctx,
            use_statements: Vec::new(),
        };
        crate::visitor::walk(&mut collector, ctx.root);

        // Early exit: if no use statements without kind, skip usage analysis
        if collector.use_statements.is_empty() {
            return Vec::new();
        }

        // Analyze usage patterns
        let mut usage_analyzer = UsageAnalyzer {
            ctx,
            usage_map: HashMap::new(),
        };
        crate::visitor::walk(&mut usage_analyzer, ctx.root);

        // Generate errors with appropriate fixes
        let mut errors = Vec::new();

        for use_stmt in collector.use_statements {
            // Check if this is a grouped statement (contains |)
            let short_names: Vec<&str> = use_stmt.short_name.split('|').collect();

            // Determine usage for all names in the group
            let mut combined_usage: Option<Usage> = None;
            for short_name in &short_names {
                if let Some(&usage) = usage_analyzer.usage_map.get(*short_name) {
                    combined_usage = match combined_usage {
                        None => Some(usage),
                        Some(prev) => Some(prev.merge(usage)),
                    };
                }
            }

            let suggested_kind = match combined_usage {
                Some(Usage::TypeOnly) => Some("type"),
                Some(Usage::NamespaceOnly) => Some("namespace"),
                Some(Usage::Both) | None => None, // Don't autofix if used both ways or not used
            };

            let mut error = LintError::new(
                Severity::Warning,
                "Use `use type` or `use namespace`".to_string(),
                use_stmt.start,
                use_stmt.end,
                "use-statement-without-kind",
            );

            // Only add autofix if we can determine the kind unambiguously
            if ctx.allow_auto_fix {
                if let Some(kind) = suggested_kind {
                    let mut fix = EditSet::new();
                    fix.add(Edit::insert(use_stmt.insert_pos, format!("{} ", kind)));
                    error = error.with_fix(fix);
                }
            }

            errors.push(error);
        }

        errors
    }

    fn supports_auto_fix(&self) -> bool {
        true
    }
}

impl UseStatementWithoutKindLinter {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Usage {
    TypeOnly,
    NamespaceOnly,
    Both,
}

impl Usage {
    fn merge(self, other: Usage) -> Usage {
        match (self, other) {
            (Usage::TypeOnly, Usage::TypeOnly) => Usage::TypeOnly,
            (Usage::NamespaceOnly, Usage::NamespaceOnly) => Usage::NamespaceOnly,
            _ => Usage::Both,
        }
    }
}

struct UseStatementInfo {
    short_name: String, // Short name for lookup (last part after \)
    start: usize,
    end: usize,
    insert_pos: usize,
}

struct UseStatementCollector<'a> {
    ctx: &'a LintContext<'a>,
    use_statements: Vec<UseStatementInfo>,
}

impl<'a> SyntaxVisitor<'a> for UseStatementCollector<'a> {
    fn visit_node(&mut self, node: &'a PositionedSyntax<'a>) {
        match &node.children {
            SyntaxVariant::NamespaceUseDeclaration(use_decl) => {
                // Check if the use declaration has a kind keyword
                if !has_kind_keyword(&use_decl.kind) {
                    // Extract the imported name
                    if let Some(name) = extract_name(self.ctx, &use_decl.clauses) {
                        let short_name = get_short_name(&name);
                        let start = use_decl.kind.offset().unwrap_or(0);
                        let end = use_decl.clauses.offset().unwrap_or(start);
                        let insert_pos = use_decl.clauses.offset().unwrap_or(start);

                        self.use_statements.push(UseStatementInfo {
                            short_name,
                            start,
                            end,
                            insert_pos,
                        });
                    }
                }
            }
            SyntaxVariant::NamespaceGroupUseDeclaration(group_use) => {
                // Check if the group use declaration has a kind keyword
                if !has_kind_keyword(&group_use.kind) {
                    // For grouped use statements, we need to collect all the imported names
                    // to analyze their usage, but only report ONE error for the entire group
                    let prefix = extract_name(self.ctx, &group_use.prefix).unwrap_or_default();
                    let mut names = Vec::new();

                    // Collect all clause names
                    if let SyntaxVariant::SyntaxList(clauses) = &group_use.clauses.children {
                        for clause_node in clauses.iter() {
                            if let SyntaxVariant::ListItem(item) = &clause_node.children {
                                if let SyntaxVariant::NamespaceUseClause(clause) =
                                    &item.item.children
                                {
                                    // Check if this individual clause has a kind
                                    if !has_kind_keyword(&clause.clause_kind) {
                                        if let Some(clause_name) =
                                            extract_name(self.ctx, &clause.name)
                                        {
                                            let full_name = if prefix.is_empty() {
                                                clause_name
                                            } else {
                                                format!("{}\\{}", prefix, clause_name)
                                            };
                                            names.push(full_name);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Create a single entry for the grouped use statement
                    // We'll use the first name for lookup, but mark it as a group
                    if !names.is_empty() {
                        let start = group_use.kind.offset().unwrap_or(0);
                        let end = group_use.prefix.offset().unwrap_or(start);
                        let insert_pos = group_use.prefix.offset().unwrap_or(start);

                        // Get short names for each full name and combine them
                        let short_names: Vec<String> =
                            names.iter().map(|n| get_short_name(n)).collect();
                        let combined_short_names = short_names.join("|");

                        self.use_statements.push(UseStatementInfo {
                            short_name: combined_short_names,
                            start,
                            end,
                            insert_pos,
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

struct UsageAnalyzer<'a> {
    ctx: &'a LintContext<'a>,
    usage_map: HashMap<String, Usage>,
}

impl<'a> SyntaxVisitor<'a> for UsageAnalyzer<'a> {
    fn visit_node(&mut self, node: &'a PositionedSyntax<'a>) {
        match &node.children {
            SyntaxVariant::ObjectCreationExpression(obj_creation) => {
                // new ClassName() - type usage
                if let Some(name) = extract_name(self.ctx, &obj_creation.object) {
                    self.record_usage(name, Usage::TypeOnly);
                }
            }
            SyntaxVariant::GenericTypeSpecifier(type_spec) => {
                // Type hints - type usage
                if let Some(name) = extract_name(self.ctx, &type_spec.class_type) {
                    self.record_usage(name, Usage::TypeOnly);
                }
            }
            SyntaxVariant::SimpleTypeSpecifier(type_spec) => {
                // Type hints - type usage
                if let Some(name) = extract_name(self.ctx, &type_spec.specifier) {
                    self.record_usage(name, Usage::TypeOnly);
                }
            }
            SyntaxVariant::ScopeResolutionExpression(scope_res) => {
                // ClassName::method() - type usage
                if let Some(name) = extract_name(self.ctx, &scope_res.qualifier) {
                    self.record_usage(name, Usage::TypeOnly);
                }
            }
            SyntaxVariant::FunctionCallExpression(func_call) => {
                // Check if it's a qualified function call (namespace\func())
                if let Some(name) = extract_qualified_namespace(self.ctx, &func_call.receiver) {
                    self.record_usage(name, Usage::NamespaceOnly);
                }
            }
            _ => {}
        }
    }
}

impl<'a> UsageAnalyzer<'a> {
    fn record_usage(&mut self, name: String, usage: Usage) {
        self.usage_map
            .entry(name)
            .and_modify(|u| *u = u.merge(usage))
            .or_insert(usage);
    }
}

/// Check if the syntax node represents a kind keyword (type, namespace, function, const)
fn has_kind_keyword<'a>(node: &PositionedSyntax<'a>) -> bool {
    // If the node is missing, there's no kind keyword
    match &node.children {
        SyntaxVariant::Missing => false,
        SyntaxVariant::Token(token) => {
            // Check if it's one of the kind keywords
            matches!(
                token.kind(),
                TokenKind::Type | TokenKind::Namespace | TokenKind::Function | TokenKind::Const
            )
        }
        _ => {
            // Could be some other node, let's check if it contains a kind keyword
            for child in node.iter_children() {
                if has_kind_keyword(child) {
                    return true;
                }
            }
            false
        }
    }
}

/// Extract a simple name from a qualified name node
fn extract_name<'a>(ctx: &LintContext<'a>, node: &PositionedSyntax<'a>) -> Option<String> {
    let text = ctx.node_text(node).trim();

    if text.is_empty() {
        return None;
    }

    match &node.children {
        SyntaxVariant::Token(_) => Some(text.to_string()),
        SyntaxVariant::QualifiedName(_) => {
            // Get the full qualified name text and clean it up
            let name = text.replace("\\\\", "\\").replace(" ", "");
            Some(name)
        }
        SyntaxVariant::SimpleTypeSpecifier(spec) => extract_name(ctx, &spec.specifier),
        _ => Some(text.to_string()),
    }
}

/// Extract the namespace part from a qualified function call
fn extract_qualified_namespace<'a>(
    ctx: &LintContext<'a>,
    node: &PositionedSyntax<'a>,
) -> Option<String> {
    match &node.children {
        SyntaxVariant::QualifiedName(_) => {
            let text = ctx.node_text(node).trim();
            // Split by backslash and remove the last part (function name)
            let parts: Vec<&str> = text.split('\\').filter(|s| !s.is_empty()).collect();
            if parts.len() > 1 {
                Some(parts[..parts.len() - 1].join("\\"))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Get the short name (alias) from a fully qualified name
/// E.g., "Foo\Bar\Baz" => "Baz"
fn get_short_name(full_name: &str) -> String {
    full_name
        .split('\\')
        .last()
        .unwrap_or(full_name)
        .to_string()
}
