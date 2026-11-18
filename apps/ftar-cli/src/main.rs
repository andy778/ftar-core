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
  tools      list the JSON automation tools (name, description, schema)
  call       ftar call <tool> [json-args]    run one tool, print the JSON result
  run        ftar run <script.json>          run a JSON script (steps of {tool, args})
  mcp        ftar mcp --root DIR             MCP server on stdio (opt-in); --unrestricted allows any path

options:
  --order N    use cut sets up to order N (default: all)
  --terms N    inclusion-exclusion terms for `prob` (default: 5)
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
    if args[0] == "mcp" {
        return mcp(&args[1..]);
    }
    if matches!(args[0].as_str(), "tools" | "call" | "run") {
        return automation(&args);
    }
    if args.len() < 2 {
        print!("{USAGE}");
        return Err("missing arguments".into());
    }
    let (cmd, file) = (&args[0], &args[1]);
    let mut order = usize::MAX;
    let mut terms = 5usize;
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
                println!(
                    "{:>4}  {}",
                    i + 1,
                    s.iter().cloned().collect::<Vec<_>>().join(" ")
                );
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
                println!(
                    "{:>4}  {:<40} {:E}",
                    i + 1,
                    cs.iter().cloned().collect::<Vec<_>>().join(" "),
                    p
                );
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
            let name = std::path::Path::new(file)
                .file_stem()
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
