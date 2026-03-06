use crate::config::Config;
use hakana_code_info::analysis_result::AnalysisResult;
use hakana_code_info::function_complexity::FunctionComplexity;
use hakana_code_info::function_context::FunctionLikeIdentifier;
use hakana_code_info::functionlike_info::FunctionLikeInfo;
use hakana_str::Interner;
use oxidized::aast;
use oxidized::ast_defs::Bop;

/// Calculate cyclomatic complexity for a statement
fn calculate_stmt_complexity(stmt: &aast::Stmt<(), ()>) -> u32 {
    let mut complexity = 0;

    match &stmt.1 {
        aast::Stmt_::If(boxed) => {
            complexity += 1;
            complexity += count_expr_complexity(&boxed.0);
            for s in &(boxed.1).0 {
                complexity += calculate_stmt_complexity(s);
            }
            for s in &(boxed.2).0 {
                complexity += calculate_stmt_complexity(s);
            }
        }
        aast::Stmt_::While(boxed) => {
            complexity += 1;
            complexity += count_expr_complexity(&boxed.0);
            for s in &(boxed.1).0 {
                complexity += calculate_stmt_complexity(s);
            }
        }
        aast::Stmt_::Do(boxed) => {
            complexity += 1;
            for s in &(boxed.0).0 {
                complexity += calculate_stmt_complexity(s);
            }
            complexity += count_expr_complexity(&boxed.1);
        }
        aast::Stmt_::For(boxed) => {
            complexity += 1;
            for s in &(boxed.3).0 {
                complexity += calculate_stmt_complexity(s);
            }
        }
        aast::Stmt_::Foreach(boxed) => {
            complexity += 1;
            for s in &(boxed.2).0 {
                complexity += calculate_stmt_complexity(s);
            }
        }
        aast::Stmt_::Switch(boxed) => {
            for case in &boxed.1 {
                complexity += 1;
                for s in &(case.1).0 {
                    complexity += calculate_stmt_complexity(s);
                }
            }
            if let Some(default_case) = &boxed.2 {
                for s in &(default_case.1).0 {
                    complexity += calculate_stmt_complexity(s);
                }
            }
        }
        aast::Stmt_::Try(boxed) => {
            for s in &(boxed.0).0 {
                complexity += calculate_stmt_complexity(s);
            }
            for catch in &boxed.1 {
                complexity += 1;
                for s in &(catch.2).0 {
                    complexity += calculate_stmt_complexity(s);
                }
            }
            for s in &(boxed.2).0 {
                complexity += calculate_stmt_complexity(s);
            }
        }
        aast::Stmt_::Block(boxed) => {
            for s in &(boxed.1).0 {
                complexity += calculate_stmt_complexity(s);
            }
        }
        aast::Stmt_::Expr(boxed) => {
            complexity += count_expr_complexity(boxed);
        }
        aast::Stmt_::Return(boxed) => {
            if let Some(expr) = boxed.as_ref() {
                complexity += count_expr_complexity(expr);
            }
        }
        aast::Stmt_::Throw(boxed) => {
            complexity += count_expr_complexity(boxed);
        }
        _ => {}
    }

    complexity
}

/// Count complexity from expressions (ternary, null coalesce, && and ||)
fn count_expr_complexity(expr: &aast::Expr<(), ()>) -> u32 {
    let mut complexity = 0;

    match &expr.2 {
        aast::Expr_::Binop(boxed) => {
            match &boxed.bop {
                Bop::Ampamp | Bop::Barbar => {
                    complexity += 1;
                }
                Bop::QuestionQuestion => {
                    complexity += 1;
                }
                _ => {}
            }
            complexity += count_expr_complexity(&boxed.lhs);
            complexity += count_expr_complexity(&boxed.rhs);
        }
        aast::Expr_::Eif(boxed) => {
            complexity += 1;
            complexity += count_expr_complexity(&boxed.0);
            if let Some(ref then_expr) = boxed.1 {
                complexity += count_expr_complexity(then_expr);
            }
            complexity += count_expr_complexity(&boxed.2);
        }
        aast::Expr_::Call(boxed) => {
            for arg in &boxed.args {
                let arg_expr = match arg {
                    aast::Argument::Ainout(_, e) => e,
                    aast::Argument::Anormal(e) => e,
                    aast::Argument::Anamed(_, e) => e,
                };
                complexity += count_expr_complexity(arg_expr);
            }
        }
        aast::Expr_::Tuple(exprs) => {
            for e in exprs {
                complexity += count_expr_complexity(e);
            }
        }
        aast::Expr_::List(exprs) => {
            for e in exprs {
                complexity += count_expr_complexity(e);
            }
        }
        _ => {}
    }

    complexity
}

pub(crate) fn analyze(
    config: &Config,
    interner: &Interner,
    functionlike_id: FunctionLikeIdentifier,
    functionlike_storage: &FunctionLikeInfo,
    stmts: &Vec<aast::Stmt<(), ()>>,
    analysis_result: &mut AnalysisResult,
) {
    if config.analyze_cyclomatic_complexity
        && (config.cyclomatic_complexity_file_patterns.is_empty()
            || config
                .cyclomatic_complexity_file_patterns
                .iter()
                .any(|pattern| {
                    pattern.matches(interner.lookup(&functionlike_storage.def_location.file_path.0))
                }))
    {
        let mut complexity = 1; // Base complexity

        for stmt in stmts {
            complexity += calculate_stmt_complexity(stmt);
        }

        if complexity > config.cyclomatic_complexity_threshold {
            analysis_result
                .cyclomatic_complexity
                .push(FunctionComplexity::from_functionlike(
                    &functionlike_id,
                    functionlike_storage,
                    complexity,
                ));
        }
    }
}
