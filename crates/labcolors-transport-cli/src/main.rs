mod app;

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let code = app::run(
        std::env::args_os().skip(1),
        stdin.lock(),
        stdout.lock(),
        stderr.lock(),
    );
    std::process::exit(code);
}
