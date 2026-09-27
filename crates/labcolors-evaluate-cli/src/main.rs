mod app;
mod request;
mod sink;

fn main() {
    let (stdin, stdout, stderr) = (std::io::stdin(), std::io::stdout(), std::io::stderr());
    std::process::exit(app::run(std::env::args_os().skip(1), stdin.lock(), stdout.lock(), stderr.lock()));
}
