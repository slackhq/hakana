fn main() {
    hakana_cli::init(
        vec![],
        vec![],
        env!("VERGEN_GIT_SHA"),
        Box::new(hakana_cli::test_runners::core_test_runner::CoreTestRunner {}),
    );
}
