fn main() {
    std::process::exit(remtui::fake::remctl::run(std::env::args().collect()))
}
