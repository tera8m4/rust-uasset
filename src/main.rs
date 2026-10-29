use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rust_uasset::Asset;
use rust_uasset::render::Options;

const HELP: &str = "\
uasset - dump Unreal Engine .uasset/.umap files as LLM friendly text

USAGE:
    uasset [OPTIONS] <PATH>...

PATH may be a file or a directory (searched recursively for .uasset/.umap).

OPTIONS:
    -o, --output <FILE>     Write output to FILE instead of stdout
        --json              Emit JSON (one document per asset, JSON lines for many assets)
    -v, --verbose           Show everything: pin types, node positions, registry tags,
                            undecoded data, metadata
        --summary           Only print the header, asset registry tags and dependencies
        --imports           Print the import table
        --exports           Print the export table
        --nodes             Print graphs as numbered node listings instead of pseudocode
        --no-other          Do not print objects that are not part of the main view
        --max-string <N>    Shorten strings longer than N characters (default 2000, 0 = no limit)
    -h, --help              Print help
    -V, --version           Print version
";

struct Args {
    paths: Vec<PathBuf>,
    output: Option<PathBuf>,
    json: bool,
    opts: Options,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args { paths: vec![], output: None, json: false, opts: Options::default() };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("uasset {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-o" | "--output" => a.output = Some(it.next().ok_or("--output needs a value")?.into()),
            "--json" => a.json = true,
            "-v" | "--verbose" => a.opts.verbose = true,
            "--summary" => a.opts.summary_only = true,
            "--imports" => a.opts.show_imports = true,
            "--exports" => a.opts.show_exports = true,
            "--no-other" => a.opts.show_other_objects = false,
            "--nodes" => a.opts.nodes = true,
            "--max-string" => {
                a.opts.max_string = it.next().ok_or("--max-string needs a value")?.parse().map_err(|_| "--max-string needs a number")?
            }
            s if s.starts_with('-') && s.len() > 1 => return Err(format!("unknown option {s}")),
            _ => a.paths.push(arg.into()),
        }
    }
    if a.paths.is_empty() {
        return Err("no input files".into());
    }
    Ok(a)
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let mut entries: Vec<PathBuf> = match std::fs::read_dir(path) {
            Ok(rd) => rd.flatten().map(|e| e.path()).collect(),
            Err(_) => return,
        };
        entries.sort();
        for p in entries {
            if p.is_dir() {
                collect(&p, out);
            } else if matches!(p.extension().and_then(|e| e.to_str()), Some("uasset") | Some("umap")) {
                out.push(p);
            }
        }
    } else {
        out.push(path.to_path_buf());
    }
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}\n\n{HELP}");
            return ExitCode::from(2);
        }
    };
    let mut files = vec![];
    for p in &args.paths {
        collect(p, &mut files);
    }
    let mut sink: Box<dyn Write> = match &args.output {
        Some(p) => match std::fs::File::create(p) {
            Ok(f) => Box::new(std::io::BufWriter::new(f)),
            Err(e) => {
                eprintln!("error: cannot create {}: {e}", p.display());
                return ExitCode::FAILURE;
            }
        },
        None => Box::new(std::io::BufWriter::new(std::io::stdout().lock())),
    };
    let mut failed = 0;
    for (i, f) in files.iter().enumerate() {
        let res = Asset::open(f);
        let text = match res {
            Ok(asset) => {
                if args.json {
                    asset.to_json(&f.display().to_string())
                } else {
                    asset.to_text(&args.opts)
                }
            }
            Err(e) => {
                failed += 1;
                if args.json {
                    format!("{{\"file\":{},\"error\":{}}}", rust_uasset::render::json::quote(&f.display().to_string()), rust_uasset::render::json::quote(&e.to_string()))
                } else {
                    format!("# {}\n!! failed to parse: {e}\n", f.display())
                }
            }
        };
        if !args.json && files.len() > 1 {
            if i > 0 {
                let _ = writeln!(sink);
            }
            let _ = writeln!(sink, "<!-- file: {} -->", f.display());
        }
        let _ = sink.write_all(text.as_bytes());
        if args.json {
            let _ = writeln!(sink);
        }
    }
    let _ = sink.flush();
    if failed > 0 && failed == files.len() { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
