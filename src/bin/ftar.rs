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

options:
  --order N    use cut sets up to order N (default: all)
  --terms N    inclusion-exclusion terms for `prob` (default: 5)
  --time T     unit time for failure-rate events (default: 1)
  --trials N   Monte Carlo trials (default: 100000)
  --seed N     Monte Carlo seed
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
    let mut i = 2;
    while i < args.len() {
        let v = args.get(i + 1).ok_or_else(|| format!("{} needs a value", args[i]))?;
        match args[i].as_str() {
            "--order" => order = v.parse().map_err(|_| format!("bad value `{v}` for {}", args[i]))?,
            "--terms" => terms = v.parse().map_err(|_| format!("bad value `{v}` for {}", args[i]))?,
            "--time" => time = v.parse().map_err(|_| format!("bad value `{v}` for {}", args[i]))?,
            "--trials" => trials = v.parse().map_err(|_| format!("bad value `{v}` for {}", args[i]))?,
            "--seed" => seed = v.parse().map_err(|_| format!("bad value `{v}` for {}", args[i]))?,
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
            println!("nodes: {}  gates: {}  primary events: {}  database events: {}",
                nodes.len(), gates, prim, proj.db.events.len());
            if !proj.tree.note.is_empty() {
                println!("note: {}", proj.tree.note.replace('\n', " "));
            }
        }
        "cutsets" => {
            let sets = minimal_cut_sets_limited(&proj.tree, order).map_err(|e| e.to_string())?;
            println!("{} minimal cut sets", sets.len());
            for (i, s) in sets.iter().enumerate() {
                println!("{:>4}  {}", i + 1, s.iter().cloned().collect::<Vec<_>>().join(" "));
            }
        }
        "prob" => {
            let r = probability_report(&proj.tree, &proj.db,
                &ProbabilityOptions { max_order: order, terms, unit_time: time })
                .map_err(|e| e.to_string())?;
            println!("Minimal cut set probabilities:");
            for (i, (cs, p)) in r.cut_sets.iter().enumerate() {
                println!("{:>4}  {:<40} {:E}", i + 1, cs.iter().cloned().collect::<Vec<_>>().join(" "), p);
            }
            println!("\nProbability of top level event:");
            for (k, (t, s)) in r.terms.iter().zip(&r.partial_sums).enumerate() {
                let bound = if k % 2 == 0 { "upper" } else { "lower" };
                println!("{:>2} term{}  {}{:E} = {:E} ({bound} bound)", k + 1,
                    if k > 0 { "s" } else { " " }, if *t >= 0.0 { "+" } else { "-" }, t.abs(), s);
            }
            if r.exact {
                println!("\nExact value : {:E}", r.top);
            }
            println!("\nPrimary event analysis:\n {:<15} {:<20} Importance", "Event", "Failure contrib.");
            for e in &r.events {
                println!(" {:<15} {:<20E} {:5.2}%", e.id, e.contribution, e.importance_pct);
            }
        }
        "mc" => {
            let r = simulate(&proj.tree, &proj.db, &MonteCarloOptions { trials, unit_time: time, seed })
                .map_err(|e| e.to_string())?;
            println!("Number of tests           = {}", r.trials);
            println!("Number of system failures = {}", r.system_failures);
            println!("P(at least one component failure) = {:E} (exact)", r.any_component_failure);
            println!("P(top event)              = {:E} (+/- {:E})", r.probability, r.std_error);
            println!("\nCut sets by hits:");
            let mut h = r.cut_set_hits.clone();
            h.sort_by_key(|x| std::cmp::Reverse(x.1));
            for (cs, n) in h {
                println!("{:>8}  {}", n, cs.iter().cloned().collect::<Vec<_>>().join(" "));
            }
        }
        c => return Err(format!("unknown command `{c}`\n\n{USAGE}")),
    }
    Ok(())
}
