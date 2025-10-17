use super::test_runner::HooksProvider;

pub struct CoreHooksProvider {}

impl HooksProvider for CoreHooksProvider {
    fn get_hooks_for_test(
        &self,
        _: &str,
    ) -> Vec<Box<dyn hakana_analyzer::custom_hook::CustomHook>> {
        vec![]
    }

    fn get_linters_for_test(&self, dir: &str) -> Vec<Box<dyn hakana_lint::Linter>> {
        use hakana_lint::examples;

        // Build list of all core linters
        let mut all_linters: Vec<Box<dyn hakana_lint::Linter>> = vec![
            Box::new(examples::must_use_braces_for_control_flow::MustUseBracesForControlFlowLinter),
            Box::new(examples::dont_discard_new_expressions::DontDiscardNewExpressionsLinter),
            Box::new(examples::no_empty_statements::NoEmptyStatementsLinter),
            Box::new(examples::no_whitespace_at_end_of_line::NoWhitespaceAtEndOfLineLinter),
            Box::new(examples::use_statement_without_kind::UseStatementWithoutKindLinter),
        ];

        // Filter to linters that match the directory name
        all_linters.retain(|linter| {
            if let Some(hhast_name) = linter.hhast_name() {
                // For HHAST-compatible linters, check if directory contains the linter name
                dir.contains(hhast_name.split('\\').last().unwrap_or(""))
            } else {
                false
            }
        });

        all_linters
    }
}
