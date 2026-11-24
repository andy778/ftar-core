use std::process::ExitCode;

use ftar_core::analysis::{minimal_cut_sets_limited, probability_report, ProbabilityOptions};
use ftar_core::montecarlo::{simulate, MonteCarloOptions};
use ftar_core::project::Project;

const USAGE: &str = "\
ftar - fault tree analysis (OpenFTA-compatible)

usage: ftar <command> <tree.fta> [options]

commands:
  info       tree summary
  cutsets    minimal cut sets
  prob       cut-set probabilities, top-event probability, event importance
  mc         Monte Carlo simulation
  draw       ftar draw <tree.fta> <out.png|out.svg|out.html> [--scale S] [page options]   draw the tree
  bundle     ftar bundle <tree.fta> [out.zip]      zip a tree with its database, transfer-in trees and reports (for bug reports)
  unbundle   ftar unbundle <bundle.zip> [dir]      unpack a bundle (default dir: the zip's name); never overwrites
  tools      list the JSON automation tools (name, description, schema)
  call       ftar call <tool> [json-args]    run one tool, print the JSON result
  run        ftar run <script.json>          run a JSON script (steps of {tool, args})
  mcp        ftar mcp --root DIR             MCP server on stdio (opt-in); --unrestricted allows any path

options:
  --order N    use cut sets up to order N (default: all)
  --terms N    inclusion-exclusion terms for `prob` (default: 16)
  --time T     unit time for failure-rate events (default: 1)
  --trials N   Monte Carlo trials (default: 100000)
  --seed N     Monte Carlo seed
  --method M   Monte Carlo: first (default, OpenFTA fast method) or direct
  --root DIR   (call/run) confine all file access to DIR
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(m) => {
            eprintln!("ftar: {m}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return Ok(());
    }
    let Some((cmd, rest)) = args.split_first() else {
        eprint!("{USAGE}");
        return Err("missing command".into());
    };
    match cmd.as_str() {
        "draw" => return draw(rest),
        "bundle" => return bundle(rest),
        "unbundle" => return unbundle(rest),
        "mcp" => return mcp(rest),
        "tools" | "call" | "run" => return automation(&args),
        _ => {}
    }
    let Some(file) = rest.first() else {
        eprint!("{USAGE}");
        return Err("missing arguments".into());
    };
    let mut order = usize::MAX;
    let mut terms = ftar_core::analysis::DEFAULT_TERMS;
    let mut time = 1.0f64;
    let mut trials = 100_000u64;
    let mut seed = MonteCarloOptions::default().seed;
    let mut method = ftar_core::montecarlo::Method::FirstFailure;
    let mut i = 2;
    while i < args.len() {
        let v = args
            .get(i + 1)
            .ok_or_else(|| format!("{} needs a value", args[i]))?;
        match args[i].as_str() {
            "--order" => {
                order = v
                    .parse()
                    .map_err(|_| format!("bad value `{v}` for {}", args[i]))?
            }
            "--terms" => {
                terms = v
                    .parse()
                    .map_err(|_| format!("bad value `{v}` for {}", args[i]))?
            }
            "--time" => {
                time = v
                    .parse()
                    .map_err(|_| format!("bad value `{v}` for {}", args[i]))?
            }
            "--trials" => {
                trials = v
                    .parse()
                    .map_err(|_| format!("bad value `{v}` for {}", args[i]))?
            }
            "--method" => {
                method = match v.as_str() {
                    "first" | "first_failure" => ftar_core::montecarlo::Method::FirstFailure,
                    "direct" => ftar_core::montecarlo::Method::Direct,
                    _ => return Err(format!("bad value `{v}` for --method (first, direct)")),
                }
            }
            "--seed" => {
                seed = v
                    .parse()
                    .map_err(|_| format!("bad value `{v}` for {}", args[i]))?
            }
            o => return Err(format!("unknown option `{o}`")),
        }
        i += 2;
    }

    let proj = Project::load(file).map_err(|e| e.to_string())?;
    match cmd.as_str() {
        "info" => {
            let nodes = proj.tree.nodes();
            let prim = nodes.iter().filter(|n| n.kind.is_primary()).count();
            let gates = nodes.iter().filter(|n| n.kind.is_gate()).count();
            println!(
                "nodes: {}  gates: {}  primary events: {}  database events: {}",
                nodes.len(),
                gates,
                prim,
                proj.db.events.len()
            );
            if !proj.tree.note.is_empty() {
                println!("note: {}", proj.tree.note.replace('\n', " "));
            }
        }
        "cutsets" => {
            let sets = minimal_cut_sets_limited(&proj.tree, order).map_err(|e| e.to_string())?;
            println!("{} minimal cut sets", sets.len());
            for (i, s) in sets.iter().enumerate() {
                println!("{:>4}  {}", i + 1, s.join(" "));
            }
        }
        "prob" => {
            let r = probability_report(
                &proj.tree,
                &proj.db,
                &ProbabilityOptions {
                    max_order: order,
                    terms,
                    unit_time: time,
                },
            )
            .map_err(|e| e.to_string())?;
            println!("Minimal cut set probabilities:");
            for (i, (cs, p)) in r.cut_sets.iter().enumerate() {
                println!("{:>4}  {:<40} {:E}", i + 1, cs.join(" "), p);
            }
            println!("\nProbability of top level event:");
            for (k, (t, s)) in r.terms.iter().zip(&r.partial_sums).enumerate() {
                let bound = if k % 2 == 0 { "upper" } else { "lower" };
                println!(
                    "{:>2} term{}  {}{:E} = {:E} ({bound} bound)",
                    k + 1,
                    if k > 0 { "s" } else { " " },
                    if *t >= 0.0 { "+" } else { "-" },
                    t.abs(),
                    s
                );
            }
            if r.exact {
                println!("\nExact value : {:E}", r.top);
            }
            println!(
                "\nPrimary event analysis:\n {:<15} {:<20} Importance",
                "Event", "Failure contrib."
            );
            for e in &r.events {
                println!(
                    " {:<15} {:<20E} {:5.2}%",
                    e.id, e.contribution, e.importance_pct
                );
            }
        }
        "mc" => {
            let r = simulate(
                &proj.tree,
                &proj.db,
                &MonteCarloOptions {
                    trials,
                    unit_time: time,
                    seed,
                    method,
                },
            )
            .map_err(|e| e.to_string())?;
            let stats = ftar_core::report::failure_statistics_from_modes(&r);
            // OpenFTA prints the file name, extension included.
            let name = std::path::Path::new(file)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            print!(
                "{}",
                ftar_core::report::monte_carlo_report_text(&name, "", time, &r, &stats)
            );
        }
        c => return Err(format!("unknown command `{c}`\n\n{USAGE}")),
    }
    Ok(())
}

fn automation(args: &[String]) -> Result<(), String> {
    use ftar_automation::{run_script, tools, Session};
    use serde_json::Value;

    let mut root = None;
    let mut rest = Vec::new();
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        if a == "--root" {
            root = Some(it.next().ok_or("--root needs a directory")?.clone());
        } else {
            rest.push(a.clone());
        }
    }
    let mut session = match &root {
        Some(r) => Session::with_root(r).map_err(|e| e.to_string())?,
        None => Session::new(),
    };
    let print = |v: &Value| println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
    match args[0].as_str() {
        "tools" => print(&Value::Array(tools().iter().map(|t| t.to_json()).collect())),
        "call" => {
            let tool = rest.first().ok_or("usage: ftar call <tool> [json-args]")?;
            let a: Value = match rest.get(1) {
                Some(j) => serde_json::from_str(j).map_err(|e| format!("bad JSON args: {e}"))?,
                None => Value::Null,
            };
            match session.call(tool, &a) {
                Ok(v) => print(&v),
                Err(e) => {
                    print(&e.to_json());
                    return Err(e.to_string());
                }
            }
        }
        _ => {
            let file = rest
                .first()
                .ok_or("usage: ftar run <script.json> [--root DIR]")?;
            let text = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
            let script: Value = serde_json::from_str(&text).map_err(|e| format!("{file}: {e}"))?;
            let results = run_script(&mut session, &script);
            let failed = results.iter().any(|r| r.outcome.is_err());
            print(&Value::Array(results.iter().map(|r| r.to_json()).collect()));
            if failed {
                return Err("script failed".into());
            }
        }
    }
    Ok(())
}

/// `ftar mcp`: the MCP server on stdin/stdout. Opt-in, and file access must be
/// confined with `--root` unless `--unrestricted` is given explicitly.
fn mcp(args: &[String]) -> Result<(), String> {
    use ftar_automation::Session;

    let mut root = None;
    let mut unrestricted = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => root = Some(it.next().ok_or("--root needs a directory")?.clone()),
            "--unrestricted" => unrestricted = true,
            other => return Err(format!("unknown option `{other}` for `ftar mcp`")),
        }
    }
    let session = match (root, unrestricted) {
        (Some(_), true) => return Err("use either --root or --unrestricted, not both".into()),
        (Some(r), false) => Session::with_root(&r).map_err(|e| e.to_string())?,
        (None, true) => Session::new(),
        (None, false) => {
            return Err(
                "`ftar mcp` needs --root DIR (confines file access) or --unrestricted".into(),
            )
        }
    };
    eprintln!("ftar mcp: serving on stdio (JSON-RPC, one message per line)");
    let mut server = ftar_mcp::Server::new(session);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    ftar_mcp::serve(&mut server, stdin.lock(), stdout.lock()).map_err(|e| e.to_string())
}

/// `ftar draw tree.fta out.png [--scale S]`: the tree as PNG, SVG or a printable HTML page.
fn bundle(args: &[String]) -> Result<(), String> {
    let tree = std::path::Path::new(
        args.first()
            .ok_or("usage: ftar bundle <tree.fta> [out.zip]")?,
    );
    let out = args
        .get(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| ftar_bundle::default_bundle_path(tree));
    let bytes = ftar_bundle::export(tree, env!("CARGO_PKG_VERSION"))?;
    std::fs::write(&out, bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("wrote {}", out.display());
    Ok(())
}

fn unbundle(args: &[String]) -> Result<(), String> {
    let zip = std::path::Path::new(
        args.first()
            .ok_or("usage: ftar unbundle <bundle.zip> [dir]")?,
    );
    let dest = args
        .get(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| zip.with_extension(""));
    let bytes = std::fs::read(zip).map_err(|e| format!("{}: {e}", zip.display()))?;
    let got = ftar_bundle::import(&bytes, &dest)?;
    println!("unpacked {} files into {}", got.files.len(), dest.display());
    if let Some(t) = got.tree {
        println!("tree: {}", t.display());
    }
    Ok(())
}

fn draw(args: &[String]) -> Result<(), String> {
    use ftar_core::project::Document;
    use ftar_render::{print_html, svg_to_png, tree_svg, PrintOptions, SvgOptions};

    let (tree_path, out) = match args {
        [t, o, ..] => (t, o),
        _ => {
            return Err("usage: ftar draw <tree.fta> <out.png|out.svg|out.html> [--scale S] [--page-size A4|A3|A5|Letter|Legal|Tabloid] [--orientation landscape|portrait] [--fit fit|split] [--page-scale PERCENT] [--margin MM] [--sheet N]".into())
        }
    };
    let mut scale = 2.0f32;
    let (mut size, mut orient, mut fit) = (None, None, None);
    let (mut pscale, mut margin, mut sheet) = (None, None, None);
    let mut rest = args[2..].iter();
    while let Some(a) = rest.next() {
        fn num(rest: &mut std::slice::Iter<String>, what: &str) -> Result<f64, String> {
            rest.next()
                .and_then(|v| v.parse::<f64>().ok())
                .ok_or(format!("{what} needs a number"))
        }
        match a.as_str() {
            "--page-size" => size = Some(rest.next().ok_or("--page-size needs a value")?.clone()),
            "--orientation" => {
                orient = Some(rest.next().ok_or("--orientation needs a value")?.clone())
            }
            "--fit" => fit = Some(rest.next().ok_or("--fit needs a value")?.clone()),
            "--page-scale" => pscale = Some(num(&mut rest, "--page-scale")?),
            "--margin" => margin = Some(num(&mut rest, "--margin")?),
            "--sheet" => sheet = Some(num(&mut rest, "--sheet")?),
            "--scale" => {
                scale = rest
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--scale needs a number")?
            }
            other => return Err(format!("unknown option `{other}` for `ftar draw`")),
        }
    }
    let doc = Document::load(tree_path).map_err(|e| e.to_string())?;
    let name = std::path::Path::new(tree_path)
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = std::path::Path::new(out)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let setup = ftar_render::PageSetup::from_options(
        size.as_deref(),
        orient.as_deref(),
        fit.as_deref(),
        pscale,
        margin,
    )?;
    let svg_opts = SvgOptions {
        title: name.clone(),
        ..Default::default()
    };
    let drawing = |doc: &Document| -> Result<String, String> {
        match sheet {
            // one printed sheet of the page layout (print preview)
            Some(n) => {
                let pages = ftar_render::print_pages(&doc.tree, &doc.db, &name, true, &setup);
                let idx = if n >= 1.0 {
                    (n as usize).checked_sub(1)
                } else {
                    None
                };
                idx.and_then(|i| pages.get(i).cloned())
                    .ok_or(format!("--sheet must be 1..={} (got {n})", pages.len()))
            }
            None => Ok(tree_svg(&doc.tree, &doc.db, &svg_opts)),
        }
    };
    let bytes = match ext.as_str() {
        "svg" => drawing(&doc)?.into_bytes(),
        "png" => svg_to_png(&drawing(&doc)?, scale).map_err(|e| e.to_string())?,
        "html" => print_html(
            &doc.tree,
            &doc.db,
            &PrintOptions {
                title: name,
                probabilities: true,
                report: None,
                auto_print: false,
                page: setup,
            },
        )
        .into_bytes(),
        _ => return Err("output must end in .png, .svg or .html".into()),
    };
    std::fs::write(out, bytes).map_err(|e| format!("{out}: {e}"))?;
    eprintln!(
        "wrote {out} ({} sheet(s))",
        ftar_render::page_count(&doc.tree, &doc.db, true, &setup)
    );
    Ok(())
}
