use std::process::ExitCode;

use ftar_core::analysis::{minimal_cut_sets_limited, probability_report, ProbabilityOptions};
use ftar_core::montecarlo::{simulate, MonteCarloOptions};
use ftar_core::project::Project;

const USAGE: &str = "\
ftar - fault tree analysis (OpenFTA-compatible)

usage: ftar <command> <tree.fta|tree.ftt> [options]

commands:
  info       tree summary
  cutsets    minimal cut sets
  prob       cut-set probabilities, top-event probability, event importance
  mc         Monte Carlo simulation
  draw       ftar draw <tree.fta> <out.png|out.svg|out.html> [--scale S] [page options]   draw the tree
  time       ftar time <tree> [--times 0,10,100] [--samples N] [--seed S] [--order N] [--tolerance X] [--out DIR]
             time-dependent probability, intensity and rate of every gate (PFTA's analysis);
             --out writes PFTA's tables (models, events, gates, cut sets) as .tsv files
  convert    ftar convert <in> <out> [--inline]   convert between .fta (+ .ped) and the text format .ftt;
             --inline copies transfer-in trees into the output; never overwrites
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
        "convert" => return convert(rest),
        "time" => return time(rest),
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
    let o = AnalysisOptions::parse(&args[2..])?;
    let proj = Project::load(file).map_err(|e| e.to_string())?;
    match cmd.as_str() {
        "info" => print_info(&proj),
        "cutsets" => print_cut_sets(&proj, &o)?,
        "prob" => print_probability(&proj, &o)?,
        "mc" => print_monte_carlo(&proj, file, &o)?,
        c => {
            return Err(format!(
                "unknown command `{c}`

{USAGE}"
            ))
        }
    }
    Ok(())
}

/// The analysis options after `<command> <file>`.
struct AnalysisOptions {
    order: usize,
    terms: usize,
    time: f64,
    trials: u64,
    seed: u64,
    method: ftar_core::montecarlo::Method,
}

impl AnalysisOptions {
    /// `opts` is everything after the file name: `--name value` pairs.
    fn parse(opts: &[String]) -> Result<Self, String> {
        let mut o = AnalysisOptions {
            order: usize::MAX,
            terms: ftar_core::analysis::DEFAULT_TERMS,
            time: 1.0,
            trials: 100_000,
            seed: MonteCarloOptions::default().seed,
            method: ftar_core::montecarlo::Method::FirstFailure,
        };
        for pair in opts.chunks(2) {
            let (flag, v) = match pair {
                [flag, v] => (flag, v),
                [flag] => return Err(format!("{flag} needs a value")),
                _ => continue,
            };
            match flag.as_str() {
                "--order" => o.order = value(flag, v)?,
                "--terms" => o.terms = value(flag, v)?,
                "--time" => o.time = value(flag, v)?,
                "--trials" => o.trials = value(flag, v)?,
                "--method" => {
                    o.method = match v.as_str() {
                        "first" | "first_failure" => ftar_core::montecarlo::Method::FirstFailure,
                        "direct" => ftar_core::montecarlo::Method::Direct,
                        _ => return Err(format!("bad value `{v}` for --method (first, direct)")),
                    }
                }
                "--seed" => o.seed = value(flag, v)?,
                other => return Err(format!("unknown option `{other}`")),
            }
        }
        Ok(o)
    }
}

fn value<T: std::str::FromStr>(flag: &str, v: &str) -> Result<T, String> {
    v.parse().map_err(|_| format!("bad value `{v}` for {flag}"))
}

fn print_info(proj: &Project) {
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

fn print_cut_sets(proj: &Project, o: &AnalysisOptions) -> Result<(), String> {
    let sets = minimal_cut_sets_limited(&proj.tree, o.order).map_err(|e| e.to_string())?;
    println!("{} minimal cut sets", sets.len());
    for (i, s) in sets.iter().enumerate() {
        println!("{:>4}  {}", i + 1, s.join(" "));
    }
    Ok(())
}

fn print_probability(proj: &Project, o: &AnalysisOptions) -> Result<(), String> {
    let r = probability_report(
        &proj.tree,
        &proj.db,
        &ProbabilityOptions {
            max_order: o.order,
            terms: o.terms,
            unit_time: o.time,
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
    Ok(())
}

fn print_monte_carlo(proj: &Project, file: &str, o: &AnalysisOptions) -> Result<(), String> {
    let r = simulate(
        &proj.tree,
        &proj.db,
        &MonteCarloOptions {
            trials: o.trials,
            unit_time: o.time,
            seed: o.seed,
            method: o.method,
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
        ftar_core::report::monte_carlo_report_text(&name, "", o.time, &r, &stats)
    );
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

/// `ftar convert <in> <out> [--inline]`: write a tree in the format the
/// output's extension names: `.ftt` (text, tree and database in one file) or
/// `.fta` with its database as `<stem>.ped` next to it.
fn convert(args: &[String]) -> Result<(), String> {
    use ftar_core::project::Document;
    use std::path::Path;
    let mut inline = false;
    let mut files = Vec::new();
    for a in args {
        match a.as_str() {
            "--inline" => inline = true,
            o if o.starts_with("--") => {
                return Err(format!("unknown option `{o}` for `ftar convert`"))
            }
            f => files.push(f),
        }
    }
    let [input, out] = files[..] else {
        return Err("usage: ftar convert <in> <out> [--inline]".into());
    };
    let d = Document::load(input).map_err(|e| e.to_string())?;
    let (mut tree, db, extras) = if inline {
        let p = d.analysis_project().map_err(|e| e.to_string())?;
        (p.tree, p.db, d.extras)
    } else {
        (d.tree, d.db, d.extras)
    };
    let out = Path::new(out);
    let mut writes: Vec<(std::path::PathBuf, String)> = Vec::new();
    if ftar_core::text::is_text_path(out) {
        let text = ftar_core::text::write_text(&tree, &db, &extras).map_err(|e| e.to_string())?;
        writes.push((out.to_path_buf(), text));
    } else if out
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("fta"))
    {
        tree.db_name = None;
        if !db.events.is_empty() {
            let stem = out
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "tree".into());
            let ped = format!("{stem}.ped");
            writes.push((out.with_file_name(&ped), ftar_core::write::write_ped(&db)));
            tree.db_name = Some(ped);
        }
        writes.insert(0, (out.to_path_buf(), ftar_core::write::write_fta(&tree)));
    } else {
        return Err("output must end in .ftt or .fta".into());
    }
    if let Some((p, _)) = writes.iter().find(|(p, _)| p.exists()) {
        return Err(format!("{}: already exists", p.display()));
    }
    for (p, content) in &writes {
        std::fs::write(p, content).map_err(|e| format!("{}: {e}", p.display()))?;
        eprintln!("wrote {}", p.display());
    }
    Ok(())
}

/// `ftar time <tree> [options]`: PFTA's time-dependent quantification.
fn time(args: &[String]) -> Result<(), String> {
    use ftar_core::quant::{analyse_document, tsv, QuantOptions};
    let mut it = args.iter();
    let file = it.next().ok_or("usage: ftar time <tree> [options]")?;
    let mut o = QuantOptions::default();
    let mut out_dir = None;
    while let Some(flag) = it.next() {
        let v = it.next().ok_or(format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--times" => {
                let t: Result<Vec<f64>, String> =
                    v.split(',').map(|x| value("--times", x.trim())).collect();
                o.times = Some(t?);
            }
            "--samples" => o.sample_size = Some(value(flag, v)?),
            "--seed" => o.seed = Some(v.clone()),
            "--order" => o.computational_order = Some(value(flag, v)?),
            "--tolerance" => o.computational_tolerance = Some(value(flag, v)?),
            "--out" => out_dir = Some(std::path::PathBuf::from(v)),
            other => return Err(format!("unknown option `{other}` for `ftar time`")),
        }
    }
    let doc = ftar_core::project::Document::load(file).map_err(|e| e.to_string())?;
    let r = analyse_document(&doc, &o).map_err(|e| e.to_string())?;
    print_time(&r);
    if let Some(dir) = out_dir {
        let cuts = dir.join("cut-sets");
        std::fs::create_dir_all(&cuts).map_err(|e| format!("{}: {e}", cuts.display()))?;
        let mut files = vec![
            (dir.join("models.tsv"), tsv::models(&r)),
            (dir.join("events.tsv"), tsv::events(&r)),
            (dir.join("gates.tsv"), tsv::gates(&r)),
        ];
        for (i, g) in r.gates.iter().enumerate() {
            if let Some(t) = tsv::cut_sets(&r, i) {
                files.push((cuts.join(format!("{}.tsv", g.id)), t));
            }
        }
        for (p, content) in &files {
            std::fs::write(p, content).map_err(|e| format!("{}: {e}", p.display()))?;
        }
        eprintln!("wrote {} tables to {}", files.len(), dir.display());
    }
    Ok(())
}

fn print_time(r: &ftar_core::quant::QuantReport) {
    let unit = r.time_unit.as_deref().unwrap_or("");
    let per = if unit.is_empty() {
        String::new()
    } else {
        format!(" (/{unit})")
    };
    let sampled = r.sample_size > 1;
    if sampled {
        println!(
            "{} samples (seed `{}`); values are means, q also as 5% and 95% quantiles",
            r.sample_size, r.seed
        );
    }
    for g in r.gates.iter().filter(|g| g.is_top_gate || g.is_paged) {
        let label = g
            .label
            .as_deref()
            .map(|l| format!(" - {l}"))
            .unwrap_or_default();
        println!(
            "
Gate {} ({}){label}",
            g.id,
            g.gate_type.name()
        );
        let q = r.expected(&g.series.q);
        let w = r.expected(&g.series.w);
        let l = r.expected(&g.series.rate);
        let (lo, hi) = (r.quantile(&g.series.q, 0.05), r.quantile(&g.series.q, 0.95));
        print!(
            "{:>12}  {:>14}",
            format!(
                "time{}",
                if unit.is_empty() {
                    String::new()
                } else {
                    format!(" ({unit})")
                }
            ),
            "q"
        );
        if sampled {
            print!("  {:>12}  {:>12}", "q 5%", "q 95%");
        }
        println!("  {:>14}  {:>14}", format!("w{per}"), format!("rate{per}"));
        for (i, t) in r.times.iter().enumerate() {
            let at = |v: &[f64]| v.get(i).copied().unwrap_or(f64::NAN);
            print!("{:>12}  {:>14.6E}", t, at(&q));
            if sampled {
                print!("  {:>12.4E}  {:>12.4E}", at(&lo), at(&hi));
            }
            println!("  {:>14.6E}  {:>14.6E}", at(&w), at(&l));
        }
    }
}
