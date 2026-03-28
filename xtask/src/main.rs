mod download;
mod release;

#[expect(clippy::print_stderr)]
fn main() {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(String::as_str) {
        Some("download-fixtures") => download::run(),
        Some("release") => release::run(&args[2..]),
        Some(cmd) => {
            eprintln!("unknown command: {cmd}");
            panic!("unknown xtask command");
        }
        None => {
            eprintln!(
                "usage: cargo xtask <command>\n\n\
                 commands:\n  \
                 download-fixtures  Download pixelmatch test fixtures\n  \
                 release            Release management (update, publish, changelog)"
            );
            panic!("no xtask command specified");
        }
    }
}
