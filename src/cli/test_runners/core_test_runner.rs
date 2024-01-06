use super::test_runner::HooksProvider;

pub struct CoreHooksProvider {}

impl HooksProvider for CoreHooksProvider {
    fn get_hooks_for_test(
        &self,
        _: &str,
    ) -> Vec<Box<dyn hakana_analyzer::custom_hook::CustomHook>> {
        vec![]
    }
}
