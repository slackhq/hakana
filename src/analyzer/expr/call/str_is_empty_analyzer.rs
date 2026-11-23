use crate::{
    function_analysis_data::FunctionAnalysisData, scope::BlockContext,
    scope_analyzer::ScopeAnalyzer, statements_analyzer::StatementsAnalyzer,
};
use hakana_code_info::{
    analysis_result::Replacement,
    issue::{Issue, IssueKind},
    t_atomic::TAtomic,
};
use hakana_parser::aast::{self, Pos};

/// Reports calls to `HH\Lib\Str\is_empty()`, which is being removed from the HSL.
///
/// When the argument is known to be a string the call is the same as comparing
/// it with `''`, and that is what the autofix writes: `Str\is_empty($s)` becomes
/// `$s === ''` and `!Str\is_empty($s)` becomes `$s !== ''`. A nullable argument
/// is only reported, since whoever fixes it has to decide what null means there.
pub(super) fn check(
    statements_analyzer: &StatementsAnalyzer,
    args: &[aast::Argument],
    unpacked_arg: &Option<aast::Expr>,
    pos: &Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    let ([aast::Argument::Anormal(arg)], None) = (args, unpacked_arg) else {
        return;
    };

    let Some(arg_type) = analysis_data.get_expr_type(arg.pos()) else {
        return;
    };

    let is_string = !arg_type.types.is_empty()
        && arg_type
            .types
            .iter()
            .all(|t| t.is_string() || t.is_string_subtype());

    let message = if is_string {
        "HH\\Lib\\Str\\is_empty() is going away, compare the string with '' instead".to_string()
    } else if arg_type.types.iter().any(|t| matches!(t, TAtomic::TNull)) {
        format!(
            "HH\\Lib\\Str\\is_empty() is going away, and its argument has the nullable type {}: \
            decide what null means here and compare with null and '' explicitly",
            arg_type.get_id(Some(statements_analyzer.interner))
        )
    } else {
        format!(
            "HH\\Lib\\Str\\is_empty() is going away, and its argument has the type {}, \
            which is not known to be a string",
            arg_type.get_id(Some(statements_analyzer.interner))
        )
    };

    let issue = Issue::new(
        IssueKind::StrIsEmptyCall,
        message,
        statements_analyzer.get_hpos(pos),
        &context.function_context.calling_functionlike_id,
    );

    if is_string && statements_analyzer.should_autofix(context, analysis_data, &issue) {
        let contents = &statements_analyzer.file_analyzer.file_source.file_contents;
        let Some(arg_text) = contents.get(arg.pos().start_offset()..arg.pos().end_offset()) else {
            return;
        };

        let arg_text = if is_comparison_operand(&arg.2) {
            arg_text.to_string()
        } else {
            format!("({})", arg_text)
        };

        let mut start = pos.start_offset();
        let end = pos.end_offset();

        // `!Str\is_empty($s)` becomes `$s !== ''` rather than `!($s === '')`
        let before = contents[..start].trim_end();
        let negated = before.ends_with('!');
        if negated {
            start = before.len() - 1;
        }

        let comparison = format!("{} {} ''", arg_text, if negated { "!==" } else { "===" });

        let replacement = if needs_no_parentheses(&contents[..start], &contents[end..]) {
            comparison
        } else {
            format!("({})", comparison)
        };

        analysis_data.add_replacement(
            (start as u32, end as u32),
            Replacement::Substitute(replacement),
        );
    } else {
        analysis_data.maybe_add_issue(
            issue,
            statements_analyzer.get_config(),
            statements_analyzer.get_file_path_actual(),
        );
    }
}

/// Whether the expression binds tighter than `===`, so it can be its operand as written.
fn is_comparison_operand(expr: &aast::Expr_) -> bool {
    match expr {
        aast::Expr_::Lvar(_)
        | aast::Expr_::Id(_)
        | aast::Expr_::This
        | aast::Expr_::ArrayGet(_)
        | aast::Expr_::ClassGet(_)
        | aast::Expr_::ClassConst(_)
        | aast::Expr_::String(_)
        | aast::Expr_::String2(_)
        | aast::Expr_::PrefixedString(_)
        | aast::Expr_::Call(_) => true,
        aast::Expr_::ObjGet(boxed) => matches!(boxed.3, aast::PropOrMethod::IsProp),
        aast::Expr_::Binop(boxed) => matches!(boxed.bop, aast::Bop::Dot),
        _ => false,
    }
}

/// Whether a comparison put between this source text needs no parentheses:
/// the call is a whole argument, condition, operand of `&&` or `||`, or the
/// value of an assignment or return.
fn needs_no_parentheses(before: &str, after: &str) -> bool {
    let before = before.trim_end();
    let after = after.trim_start();

    let safe_before = before.ends_with(['(', '[', '{', ',', ';', '?'])
        || (before.ends_with(':') && !before.ends_with("::"))
        || before.ends_with("&&")
        || before.ends_with("||")
        || before.ends_with("=>")
        || (before.ends_with('=') && !before[..before.len() - 1].ends_with(['=', '!', '<', '>']))
        || (before.ends_with("return")
            && !before[..before.len() - "return".len()]
                .ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '\\' || c == '$'));

    let safe_after = after.starts_with([')', ']', ',', ';', ':'])
        || after.starts_with("&&")
        || after.starts_with("||")
        || (after.starts_with('?') && !after.starts_with("?->") && !after.starts_with("??"));

    safe_before && safe_after
}
