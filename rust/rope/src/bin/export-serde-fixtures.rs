#[cfg(not(feature = "serde"))]
fn main() {
    eprintln!("export-serde-fixtures requires the `serde` feature to be enabled.");
    std::process::exit(1);
}

#[cfg(feature = "serde")]
use std::{env, path::PathBuf};

#[cfg(feature = "serde")]
use xi_rope::serde_fixtures::{fixtures, Fixture};

#[cfg(feature = "serde")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let mut output_dir: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dir" | "--output-dir" => {
                let value = args.next().ok_or_else(|| {
                    "--dir requires a value specifying the output directory"
                })?;
                output_dir = Some(PathBuf::from(value));
            }
            "--list" => {
                list_fixtures();
                return Ok(());
            }
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            other => {
                return Err(format!("unrecognised argument: {other}").into());
            }
        }
    }

    if let Some(dir) = output_dir {
        export_to_directory(dir.as_path(), fixtures())?;
        return Ok(());
    }

    print_usage();
    Err("missing required --dir <PATH> argument".into())
}

#[cfg(feature = "serde")]
fn print_usage() {
    eprintln!(
        "Usage: cargo run -p xi-rope --features serde --bin export-serde-fixtures -- --dir <PATH>\n       cargo run -p xi-rope --features serde --bin export-serde-fixtures -- --list"
    );
}

#[cfg(feature = "serde")]
fn list_fixtures() {
    for fixture in fixtures() {
        println!("{}", fixture.name);
    }
}

#[cfg(feature = "serde")]
fn export_to_directory(dir: &std::path::Path, fixtures: &[Fixture]) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;

    for fixture in fixtures {
        let mut content = fixture.json.to_owned();
        if !content.ends_with('\n') {
            content.push('\n');
        }
        let path = dir.join(fixture.name);
        std::fs::write(path, content)?;
    }

    Ok(())
}
