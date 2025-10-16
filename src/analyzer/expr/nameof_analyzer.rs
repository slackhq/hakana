use std::rc::Rc;

use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope_analyzer::ScopeAnalyzer;
use crate::{scope::BlockContext, statements_analyzer::StatementsAnalyzer};
use hakana_code_info::issue::{Issue, IssueKind};
use hakana_code_info::t_atomic::TAtomic;
use hakana_code_info::ttype::wrap_atomic;
use oxidized::aast;

pub(crate) fn analyze(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    expr: &aast::Expr<(), ()>,
    class_id: &Box<aast::ClassId<(), ()>>,
) {
    let resolved_nameof_type =
        resolve_class_name(statements_analyzer, analysis_data, context, expr, class_id)
            .map_or(TAtomic::TString, |value| TAtomic::TLiteralString { value });

    analysis_data.expr_types.insert(
        (expr.1.start_offset() as u32, expr.1.end_offset() as u32),
        Rc::new(wrap_atomic(resolved_nameof_type)),
    );
}

fn resolve_class_name(
    statements_analyzer: &StatementsAnalyzer,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    expr: &aast::Expr<(), ()>,
    class_id: &Box<aast::ClassId<(), ()>>,
) -> Option<String> {
    // The RHS of a nameof expression always seems to be a CIexpr,
    // even if a classname literal or a keyword like self/static/parent is passed.
    let aast::ClassId_::CIexpr(ci_expr) = &class_id.2 else {
        return None;
    };
    let aast::Expr_::Id(inner_class_id) = &ci_expr.2 else {
        return None;
    };

    let calling_class = context.function_context.calling_class;

    match inner_class_id.name() {
        "self" | "static" | "parent" => {
            // The Hack typechecker allows nameof self/parent/static outside of a class context.
            // This would probably be an error more often than not, so disallow it.
            if calling_class.is_none() {
                analysis_data.maybe_add_issue(
                    Issue::new(
                        IssueKind::NameofUsedOutsideClassWithoutLiteral,
                        "nameof used outside of a class context with a non-literal target"
                            .to_string(),
                        statements_analyzer.get_hpos(&expr.1),
                        &context.function_context.calling_functionlike_id,
                    ),
                    statements_analyzer.get_config(),
                    statements_analyzer.get_file_path_actual(),
                );
            }

            // nameof parent in a class that has no parent is a typechecker error.
            if inner_class_id.name() == "parent" {
                calling_class
                    .and_then(|calling_class_id| {
                        statements_analyzer
                            .codebase
                            .classlike_infos
                            .get(&calling_class_id)
                    })
                    .and_then(|calling_class_info| calling_class_info.direct_parent_class)
                    .map(|id| statements_analyzer.interner.lookup(&id))
                    .map(|str| str.to_string())
            } else {
                // self/static
                calling_class
                    .map(|calling_class_id| statements_analyzer.interner.lookup(&calling_class_id))
                    .map(|str| str.to_string())
            }
        }
        // Anything else: nameof C where C is a possibly
        // namespace-relative classname literal.
        // C being invalid is already a typechecker error.
        class_name_literal => {
            if class_name_literal.starts_with("\\") {
                Some(class_name_literal[1..].to_string())
            } else {
                // Resolve namespace-relative classname references.
                statements_analyzer
                    .get_namespace()
                    .as_ref()
                    .filter(|namespace| !namespace.is_empty())
                    .map(|namespace| namespace.to_owned() + "\\" + class_name_literal)
                    .or_else(|| Some(class_name_literal.to_string()))
            }
        }
    }
}
