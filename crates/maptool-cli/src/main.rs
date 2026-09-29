use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use maptool_core::{Document, Error, GenerateOptions, Kind, Options, PixelFormat, decode_image, generate_labels, generate_mesh, vectorize_file};

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
  -h, --help                Show this help

Run `maptool generate -h` for the border-map-to-provinces subcommand.";

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

fn run_vectorize() -> ExitCode {
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

    let started = Instant::now();
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
            started.elapsed()
        );
    }
    ExitCode::SUCCESS
}

// --------------------------------------------------------- generate subcommand

const GENERATE_USAGE: &str = "\
Usage: maptool generate <input.png|input.bmp> [-o output.maptool] [options]

Turns a hand-painted border map (white = land, #00FF00 = sea/lake, black = a border
line to respect, absorbed into whichever neighbouring province is nearest) into a
province map you can open in the editor.

Options:
  -o, --output <file>          Output .maptool path (default: input with .maptool extension)
      --png <file>             Also write a color-coded province PNG, for inspection only:
                                reopening it normally loses the land/sea distinction, so
                                keep editing the .maptool file instead
      --land-radius <px>       Target land province size (default 24)
      --water-radius <px>      Target sea/lake province size, used only with --split-seas (default 24)
      --split-seas             Subdivide seas/lakes into provinces too
      --seed <n>                Change this to get a different layout (default 0)
  -t, --tolerance <px>         Simplification tolerance, 0 = exact pixel edges (default 1.0)
  -a, --corner-angle <deg>     Turns sharper than this stay corners (default 100)
  -m, --min-chain <edges>      Borders shorter than this stay unsmoothed (default 6)
  -c, --corner-run <px>        Straight sides at least this long keep sharp corners (default 4)
      --flatten-tolerance <px> How far flattened curves may stray from the true border (default 0.03)
      --no-validate            Accept single-pixel exclaves and four-way junctions
  -q, --quiet                  No progress output
  -h, --help                   Show this help";

struct GenerateArgs {
    input: PathBuf,
    output: PathBuf,
    png: Option<PathBuf>,
    gen_opts: GenerateOptions,
    mesh_opts: Options,
    flatten_tolerance: f64,
    quiet: bool,
}

fn parse_generate() -> Result<GenerateArgs, String> {
    let mut gen_opts = GenerateOptions::default();
    let mut mesh_opts = Options::default();
    let mut flatten_tolerance = 0.03;
    let mut input = None;
    let mut output = None;
    let mut png = None;
    let mut quiet = false;
    // Skip the program name and the "generate" token itself.
    let mut it = std::env::args().skip(2);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        fn num<T: std::str::FromStr>(name: &str, v: String) -> Result<T, String> {
            v.parse().map_err(|_| format!("invalid value for {name}: {v}"))
        }
        match arg.as_str() {
            "-h" | "--help" => return Err(String::new()),
            "-q" | "--quiet" => quiet = true,
            "-o" | "--output" => output = Some(PathBuf::from(value(&arg)?)),
            "--png" => png = Some(PathBuf::from(value(&arg)?)),
            "--land-radius" => gen_opts.land_radius = num(&arg, value(&arg)?)?,
            "--water-radius" => gen_opts.water_radius = num(&arg, value(&arg)?)?,
            "--split-seas" => gen_opts.split_seas = true,
            "--seed" => gen_opts.seed = num(&arg, value(&arg)?)?,
            "-t" | "--tolerance" => mesh_opts.tolerance = num(&arg, value(&arg)?)?,
            "-a" | "--corner-angle" => mesh_opts.corner_angle = num(&arg, value(&arg)?)?,
            "-m" | "--min-chain" => mesh_opts.min_chain_len = num(&arg, value(&arg)?)?,
            "-c" | "--corner-run" => mesh_opts.corner_run = num(&arg, value(&arg)?)?,
            "--flatten-tolerance" => flatten_tolerance = num(&arg, value(&arg)?)?,
            "--no-validate" => mesh_opts.validate = false,
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => {
                if input.replace(PathBuf::from(&arg)).is_some() {
                    return Err("only one input file is supported".into());
                }
            }
        }
    }
    let input = input.ok_or("missing input file")?;
    let output = output.unwrap_or_else(|| input.with_extension("maptool"));
    Ok(GenerateArgs { input, output, png, gen_opts, mesh_opts, flatten_tolerance, quiet })
}

fn run_generate() -> ExitCode {
    let args = match parse_generate() {
        Ok(a) => a,
        Err(msg) => {
            if msg.is_empty() {
                println!("{GENERATE_USAGE}");
                return ExitCode::SUCCESS;
            }
            eprintln!("error: {msg}\n\n{GENERATE_USAGE}");
            return ExitCode::from(2);
        }
    };

    let started = Instant::now();
    let bytes = match std::fs::read(&args.input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: cannot read {}: {e}", args.input.display());
            return ExitCode::FAILURE;
        }
    };
    let (rgba, w, h) = match decode_image(&bytes) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}: {e}", args.input.display());
            return ExitCode::FAILURE;
        }
    };

    let report_error = |e: &Error, path: &PathBuf| match e {
        Error::Invalid { violations, total } => {
            eprintln!("error: {}: input rejected, {total} problem(s):", path.display());
            for v in violations {
                eprintln!("  {v}");
            }
            if *total > violations.len() {
                eprintln!("  ... and {} more", total - violations.len());
            }
            eprintln!("Fix the image, or pass --no-validate to process it anyway.");
        }
        e => eprintln!("error: {}: {e}", path.display()),
    };

    let generated = match generate_mesh(&rgba, w, h, PixelFormat::Rgba, &args.gen_opts, &args.mesh_opts, args.flatten_tolerance) {
        Ok(g) => g,
        Err(e) => {
            report_error(&e, &args.input);
            return ExitCode::FAILURE;
        }
    };

    let sea = generated.sea_provinces.len();
    let land = generated.mesh.provinces.len() - sea;

    let mut doc = Document::new(generated.mesh);
    doc.provinces.set_kind(&generated.sea_provinces, Kind::Sea).expect("generated ids are always valid for the document that owns them");

    if let Err(e) = std::fs::write(&args.output, doc.to_bytes()) {
        eprintln!("error: cannot write {}: {e}", args.output.display());
        return ExitCode::FAILURE;
    }

    if let Some(png_path) = &args.png {
        let g = match generate_labels(&rgba, w, h, PixelFormat::Rgba, &args.gen_opts) {
            Ok(g) => g,
            Err(e) => {
                report_error(&e, &args.input);
                return ExitCode::FAILURE;
            }
        };
        let mut buf = Vec::with_capacity(g.data.len() * 3);
        for &id in &g.data {
            buf.extend_from_slice(&g.colors[id as usize]);
        }
        if let Err(e) = image::save_buffer(png_path, &buf, w, h, image::ColorType::Rgb8) {
            eprintln!("error: cannot write {}: {e}", png_path.display());
            return ExitCode::FAILURE;
        }
    }

    if !args.quiet {
        eprintln!(
            "{w}x{h}: {} provinces ({land} land, {sea} sea) -> {} ({:.2?})",
            land + sea,
            args.output.display(),
            started.elapsed()
        );
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("generate") {
        run_generate()
    } else {
        run_vectorize()
    }
}
