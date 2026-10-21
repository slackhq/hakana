fn main() {
    std::process::exit(hakana_format::cli::run(std::env::args().skip(1)));
}
