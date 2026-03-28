mod publish;
mod update;
mod version;

#[expect(clippy::print_stderr)]
pub fn run(args: &[String]) {
    let dry_run = args.iter().any(|a| a == "--dry-run");

    match args.first().map(String::as_str) {
        Some("update") => update::run(dry_run),
        Some("publish") => publish::run(dry_run),
        Some("changelog") => update::changelog_only(),
        Some("--dry-run" | "--help") | None => usage(),
        Some(cmd) => {
            eprintln!("unknown release command: {cmd}");
            usage();
        }
    }
}

#[expect(clippy::print_stderr)]
fn usage() {
    eprintln!(
        "usage: cargo xtask release <command> [--dry-run]\n\n\
         commands:\n  \
         update     Bump version, generate changelog, update Cargo.toml + package.json\n  \
         publish    Publish crates to crates.io in topological order\n  \
         changelog  Preview changelog without updating files"
    );
}
