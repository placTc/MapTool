use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use maptool_core::{Error, Options, vectorize_file};

const USAGE: &str = "\
Usage: maptool <input.png|input.bmp> [-o output.svg] [options]

Options:
  -o, --output <file>       Output path (default: input with .svg extension)
  -t, --tolerance <px>      Simplification tolerance, 0 = exact pixel edges (default 1.0)
  -a, --corner-angle <deg>  Turns sharper than this stay corners (default 100)
  -m, --min-chain <edges>   Borders shorter than this stay unsmoothed (default 6)
  -c, --corner-run <px>     Straight sides at least this long keep sharp corners (default 4)
      --no-validate         Accept single-pixel exclaves and four-way junctions
  -p, --precision <digits>  Decimal places in coordinates (default 2)
  -q, --quiet               No progress output
  -h, --help                Show this help";

struct Args {
    input: PathBuf,
    output: PathBuf,
    opts: Options,
    quiet: bool,
}

fn parse() -> Result<Args, String> {
    let mut opts = Options::default();
    let mut input = None;
    let mut output = None;
    let mut quiet = false;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        fn num<T: std::str::FromStr>(name: &str, v: String) -> Result<T, String> {
            v.parse().map_err(|_| format!("invalid value for {name}: {v}"))
        }
        match arg.as_str() {
            "-h" | "--help" => return Err(String::new()),
            "-q" | "--quiet" => quiet = true,
            "-o" | "--output" => output = Some(PathBuf::from(value(&arg)?)),
            "-t" | "--tolerance" => opts.tolerance = num(&arg, value(&arg)?)?,
            "-a" | "--corner-angle" => opts.corner_angle = num(&arg, value(&arg)?)?,
            "-m" | "--min-chain" => opts.min_chain_len = num(&arg, value(&arg)?)?,
            "-c" | "--corner-run" => opts.corner_run = num(&arg, value(&arg)?)?,
            "--no-validate" => opts.validate = false,
            "-p" | "--precision" => opts.precision = num(&arg, value(&arg)?)?,
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => {
                if input.replace(PathBuf::from(&arg)).is_some() {
                    return Err("only one input file is supported".into());
                }
            }
        }
    }
    let input = input.ok_or("missing input file")?;
    let output = output.unwrap_or_else(|| input.with_extension("svg"));
    Ok(Args { input, output, opts, quiet })
}

fn main() -> ExitCode {
    let args = match parse() {
        Ok(a) => a,
        Err(msg) => {
            if msg.is_empty() {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            eprintln!("error: {msg}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let t = Instant::now();
    let map = match vectorize_file(&args.input, &args.opts) {
        Ok(m) => m,
        Err(Error::Invalid { violations, total }) => {
            eprintln!("error: {}: input rejected, {total} problem(s):", args.input.display());
            for v in &violations {
                eprintln!("  {v}");
            }
            if total > violations.len() {
                eprintln!("  ... and {} more", total - violations.len());
            }
            eprintln!("Fix the image, or pass --no-validate to process it anyway.");
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("error: {}: {e}", args.input.display());
            return ExitCode::FAILURE;
        }
    };
    let svg = map.to_svg();
    if let Err(e) = std::fs::write(&args.output, &svg) {
        eprintln!("error: cannot write {}: {e}", args.output.display());
        return ExitCode::FAILURE;
    }
    if !args.quiet {
        eprintln!(
            "{}x{}: {} provinces, {:.1} MB -> {} ({:.2?})",
            map.width,
            map.height,
            map.provinces.len(),
            svg.len() as f64 / 1e6,
            args.output.display(),
            t.elapsed()
        );
    }
    ExitCode::SUCCESS
}
