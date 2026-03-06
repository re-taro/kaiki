use std::fs;
use std::path::Path;

const FIXTURES: &[&str] = &[
    "1a", "1b", "1diff", "2a", "2b", "2diff", "3a", "3b", "3diff", "4a", "4b", "4diff", "5a",
    "5b", "5diff", "6a", "6b", "6diff", "7a", "7b",
];

const BASE_URL: &str =
    "https://raw.githubusercontent.com/mapbox/pixelmatch/refs/heads/main/test/fixtures";

fn download_fixtures() {
    let dest_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("crates/kaiki_diff/fixtures");

    fs::create_dir_all(&dest_dir).expect("failed to create fixtures directory");

    let client = reqwest::blocking::Client::new();

    for name in FIXTURES {
        let filename = format!("{name}.png");
        let dest_path = dest_dir.join(&filename);

        if dest_path.exists() {
            #[expect(clippy::print_stdout)]
            {
                println!("  skip {filename} (already exists)");
            }
            continue;
        }

        let url = format!("{BASE_URL}/{filename}");
        #[expect(clippy::print_stdout)]
        {
            println!("  fetch {filename}");
        }

        let resp = client.get(&url).send().unwrap_or_else(|e| panic!("failed to fetch {url}: {e}"));

        if !resp.status().is_success() {
            panic!("HTTP {} for {url}", resp.status());
        }

        let bytes = resp.bytes().unwrap_or_else(|e| panic!("failed to read body for {url}: {e}"));
        fs::write(&dest_path, &bytes)
            .unwrap_or_else(|e| panic!("failed to write {}: {e}", dest_path.display()));
    }

    #[expect(clippy::print_stdout)]
    {
        println!("fixtures downloaded to {}", dest_dir.display());
    }
}

#[expect(clippy::print_stderr)]
fn main() {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(String::as_str) {
        Some("download-fixtures") => download_fixtures(),
        Some(cmd) => {
            eprintln!("unknown command: {cmd}");
            panic!("unknown xtask command");
        }
        None => {
            eprintln!("usage: cargo xtask <command>\n\ncommands:\n  download-fixtures  Download pixelmatch test fixtures");
            panic!("no xtask command specified");
        }
    }
}
