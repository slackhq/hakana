use clap::{Command, arg};
use rand::Rng;

use crate::test_runners::test_runner::TestRunner;

pub fn get_subcommand() -> Command<'static> {
    Command::new("test")
        .about("Runs one or more Hakana tests")
        .arg(
            arg!(--"no-cache")
                .required(false)
                .help("Whether to use cache"),
        )
        .arg(
            arg!(--"reuse-codebase")
                .required(false)
                .help("Whether to reuse codebase between tests"),
        )
        .arg(
            arg!(--"randomize")
                .required(false)
                .help("Whether to randomise test order"),
        )
        .arg(
            arg!(--"seed" <COUNT>)
                .required(false)
                .help("Seed for random test execution"),
        )
        .arg(
            arg!(--"debug")
                .required(false)
                .help("Whether to show debug output"),
        )
        .arg(
            arg!(--"repeat" <COUNT>)
                .required(false)
                .help("How many times to repeat the test (useful for profiling)"),
        )
        .arg(arg!(<TEST> "The test to run"))
        .arg_required_else_help(true)
}

pub fn handle(
    sub_matches: &clap::ArgMatches,
    test_runner: &TestRunner,
    show_progress: bool,
    had_error: &mut bool,
    header: &str,
) {
    let repeat = if let Some(val) = sub_matches.value_of("repeat").map(|f| f.to_string()) {
        val.parse::<u16>().unwrap()
    } else {
        0
    };

    let random_seed = if sub_matches.is_present("randomize") {
        if let Some(val) = sub_matches.value_of("seed").map(|f| f.to_string()) {
            Some(val.parse::<u64>().unwrap())
        } else {
            let mut rng = rand::thread_rng();
            Some(rng.r#gen())
        }
    } else {
        None
    };

    test_runner.run_test(
        sub_matches.value_of("TEST").expect("required").to_string(),
        show_progress,
        !sub_matches.is_present("no-cache"),
        sub_matches.is_present("reuse-codebase"),
        had_error,
        header,
        repeat,
        random_seed,
    );
}
