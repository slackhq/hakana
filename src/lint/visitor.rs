//! Visitor pattern for traversing the full-fidelity syntax tree

use parser_core_types::syntax_by_ref::positioned_syntax::PositionedSyntax;
use parser_core_types::syntax_by_ref::positioned_token::PositionedToken;
use parser_core_types::syntax_by_ref::positioned_value::PositionedValue;
use parser_core_types::syntax_by_ref::syntax_variant_generated::*;

/// Trait for visiting syntax nodes
///
/// Implement this trait to walk the syntax tree and perform analysis.
/// All methods have default implementations that do nothing, allowing
/// implementers to only override the nodes they care about.
#[allow(unused_variables)]
pub trait SyntaxVisitor<'a> {
    // Top-level declarations
    fn visit_script(
        &mut self,
        node: &'a ScriptChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_end_of_file(
        &mut self,
        node: &'a EndOfFileChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }

    // Namespace and imports
    fn visit_namespace_declaration(
        &mut self,
        node: &'a NamespaceDeclarationChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_namespace_use_declaration(
        &mut self,
        node: &'a NamespaceUseDeclarationChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_namespace_group_use_declaration(
        &mut self,
        node: &'a NamespaceGroupUseDeclarationChildren<
            'a,
            PositionedToken<'a>,
            PositionedValue<'a>,
        >,
    ) {
    }

    // Classes and interfaces
    fn visit_classish_declaration(
        &mut self,
        node: &'a ClassishDeclarationChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_classish_body(
        &mut self,
        node: &'a ClassishBodyChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }

    // Functions and methods
    fn visit_function_declaration(
        &mut self,
        node: &'a FunctionDeclarationChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_methodish_declaration(
        &mut self,
        node: &'a MethodishDeclarationChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_anonymous_function(
        &mut self,
        node: &'a AnonymousFunctionChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_lambda_expression(
        &mut self,
        node: &'a LambdaExpressionChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }

    // Statements
    fn visit_compound_statement(
        &mut self,
        node: &'a CompoundStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_expression_statement(
        &mut self,
        node: &'a ExpressionStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_if_statement(
        &mut self,
        node: &'a IfStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_while_statement(
        &mut self,
        node: &'a WhileStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_for_statement(
        &mut self,
        node: &'a ForStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_foreach_statement(
        &mut self,
        node: &'a ForeachStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_do_statement(
        &mut self,
        node: &'a DoStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_switch_statement(
        &mut self,
        node: &'a SwitchStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_return_statement(
        &mut self,
        node: &'a ReturnStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_throw_statement(
        &mut self,
        node: &'a ThrowStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_try_statement(
        &mut self,
        node: &'a TryStatementChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }

    // Expressions
    fn visit_binary_expression(
        &mut self,
        node: &'a BinaryExpressionChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_prefix_unary_expression(
        &mut self,
        node: &'a PrefixUnaryExpressionChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_function_call_expression(
        &mut self,
        node: &'a FunctionCallExpressionChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_member_selection_expression(
        &mut self,
        node: &'a MemberSelectionExpressionChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_variable_expression(
        &mut self,
        node: &'a VariableExpressionChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }
    fn visit_literal_expression(
        &mut self,
        node: &'a LiteralExpressionChildren<'a, PositionedToken<'a>, PositionedValue<'a>>,
    ) {
    }

    // Generic node visitor - called for every node
    fn visit_node(&mut self, node: &'a PositionedSyntax<'a>) {}

    // Generic node visitor - called for every node
    fn leave_node(&mut self, node: &'a PositionedSyntax<'a>) {}
}

/// Walk the syntax tree and call appropriate visitor methods
pub fn walk<'a, V: SyntaxVisitor<'a>>(visitor: &mut V, node: &'a PositionedSyntax<'a>) {
    visitor.visit_node(node);

    match &node.children {
        SyntaxVariant::Script(x) => {
            visitor.visit_script(x);
            walk(visitor, &x.declarations);
        }
        SyntaxVariant::EndOfFile(x) => {
            visitor.visit_end_of_file(x);
            walk(visitor, &x.token);
        }
        SyntaxVariant::NamespaceDeclaration(x) => {
            visitor.visit_namespace_declaration(x);
            walk(visitor, &x.header);
            walk(visitor, &x.body);
        }
        SyntaxVariant::NamespaceUseDeclaration(x) => {
            visitor.visit_namespace_use_declaration(x);
            walk(visitor, &x.clauses);
        }
        SyntaxVariant::NamespaceGroupUseDeclaration(x) => {
            visitor.visit_namespace_group_use_declaration(x);
            walk(visitor, &x.clauses);
        }
        SyntaxVariant::ClassishDeclaration(x) => {
            visitor.visit_classish_declaration(x);
            walk(visitor, &x.attribute);
            walk(visitor, &x.modifiers);
            walk(visitor, &x.xhp);
            walk(visitor, &x.keyword);
            walk(visitor, &x.name);
            walk(visitor, &x.type_parameters);
            walk(visitor, &x.extends_keyword);
            walk(visitor, &x.extends_list);
            walk(visitor, &x.implements_keyword);
            walk(visitor, &x.implements_list);
            walk(visitor, &x.body);
        }
        SyntaxVariant::ClassishBody(x) => {
            visitor.visit_classish_body(x);
            walk(visitor, &x.elements);
        }
        SyntaxVariant::FunctionDeclaration(x) => {
            visitor.visit_function_declaration(x);
            walk(visitor, &x.attribute_spec);
            walk(visitor, &x.declaration_header);
            walk(visitor, &x.body);
        }
        SyntaxVariant::MethodishDeclaration(x) => {
            visitor.visit_methodish_declaration(x);
            walk(visitor, &x.attribute);
            walk(visitor, &x.function_decl_header);
            walk(visitor, &x.function_body);
        }
        SyntaxVariant::AnonymousFunction(x) => {
            visitor.visit_anonymous_function(x);
            walk(visitor, &x.body);
        }
        SyntaxVariant::LambdaExpression(x) => {
            visitor.visit_lambda_expression(x);
            walk(visitor, &x.signature);
            walk(visitor, &x.body);
        }
        SyntaxVariant::CompoundStatement(x) => {
            visitor.visit_compound_statement(x);
            walk(visitor, &x.statements);
        }
        SyntaxVariant::ExpressionStatement(x) => {
            visitor.visit_expression_statement(x);
            walk(visitor, &x.expression);
        }
        SyntaxVariant::IfStatement(x) => {
            visitor.visit_if_statement(x);
            walk(visitor, &x.condition);
            walk(visitor, &x.statement);
            walk(visitor, &x.else_clause);
        }
        SyntaxVariant::WhileStatement(x) => {
            visitor.visit_while_statement(x);
            walk(visitor, &x.condition);
            walk(visitor, &x.body);
        }
        SyntaxVariant::ForStatement(x) => {
            visitor.visit_for_statement(x);
            walk(visitor, &x.initializer);
            walk(visitor, &x.control);
            walk(visitor, &x.end_of_loop);
            walk(visitor, &x.body);
        }
        SyntaxVariant::ForeachStatement(x) => {
            visitor.visit_foreach_statement(x);
            walk(visitor, &x.collection);
            walk(visitor, &x.key);
            walk(visitor, &x.value);
            walk(visitor, &x.body);
        }
        SyntaxVariant::DoStatement(x) => {
            visitor.visit_do_statement(x);
            walk(visitor, &x.body);
            walk(visitor, &x.condition);
        }
        SyntaxVariant::SwitchStatement(x) => {
            visitor.visit_switch_statement(x);
            walk(visitor, &x.expression);
            walk(visitor, &x.sections);
        }
        SyntaxVariant::ReturnStatement(x) => {
            visitor.visit_return_statement(x);
            walk(visitor, &x.expression);
        }
        SyntaxVariant::ThrowStatement(x) => {
            visitor.visit_throw_statement(x);
            walk(visitor, &x.expression);
        }
        SyntaxVariant::TryStatement(x) => {
            visitor.visit_try_statement(x);
            walk(visitor, &x.compound_statement);
            walk(visitor, &x.catch_clauses);
            walk(visitor, &x.finally_clause);
        }
        SyntaxVariant::BinaryExpression(x) => {
            visitor.visit_binary_expression(x);
            walk(visitor, &x.left_operand);
            walk(visitor, &x.operator);
            walk(visitor, &x.right_operand);
        }
        SyntaxVariant::PrefixUnaryExpression(x) => {
            visitor.visit_prefix_unary_expression(x);
            walk(visitor, &x.operator);
            walk(visitor, &x.operand);
        }
        SyntaxVariant::FunctionCallExpression(x) => {
            visitor.visit_function_call_expression(x);
            walk(visitor, &x.receiver);
            walk(visitor, &x.argument_list);
        }
        SyntaxVariant::MemberSelectionExpression(x) => {
            visitor.visit_member_selection_expression(x);
            walk(visitor, &x.object);
            walk(visitor, &x.name);
        }
        SyntaxVariant::VariableExpression(x) => {
            visitor.visit_variable_expression(x);
            walk(visitor, &x.expression);
        }
        SyntaxVariant::LiteralExpression(x) => {
            visitor.visit_literal_expression(x);
            walk(visitor, &x.expression);
        }
        SyntaxVariant::SyntaxList(nodes) => {
            for child in nodes.iter() {
                walk(visitor, child);
            }
        }
        SyntaxVariant::Missing => {}
        SyntaxVariant::Token(_) => {}
        // For all other variants, recursively walk children
        _ => {
            for child in node.iter_children() {
                walk(visitor, child);
            }
        }
    }

    visitor.leave_node(node);
}
