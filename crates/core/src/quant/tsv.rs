//! The tables of a [`QuantReport`] as tab-separated text, with PFTA's
//! columns: `models.tsv`, `events.tsv`, `gates.tsv`, and one cut-set table
//! and one importance table per gate. Numbers are written as Python writes them (`0.1`, `1e-05`,
//! `nan`, `inf`), booleans as `True`/`False`, missing values as empty.

use super::{QuantReport, Series};

/// A float as Python's `repr` writes it.
pub fn py_float(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.into();
    }
    let a = x.abs();
    if a != 0.0 && !(1e-4..1e16).contains(&a) {
        // Rust gives `1e-5`; Python `1e-05`.
        let s = format!("{x:e}");
        return match s.split_once('e') {
            Some((m, e)) => {
                let (sign, digits) = match e.strip_prefix('-') {
                    Some(d) => ('-', d),
                    None => ('+', e),
                };
                format!("{m}e{sign}{digits:0>2}")
            }
            None => s,
        };
    }
    let s = format!("{x}");
    if s.contains('.') {
        s
    } else {
        format!("{s}.0")
    }
}

fn py_bool(b: bool) -> &'static str {
    if b {
        "True"
    } else {
        "False"
    }
}

/// A text cell: tabs and line breaks become spaces, so a row stays a row.
fn cell(s: Option<&str>) -> String {
    s.unwrap_or("").replace(['\t', '\n', '\r'], " ")
}

fn row(out: &mut String, cells: &[String]) {
    out.push_str(&cells.join("\t"));
    out.push('\n');
}

/// `(time, sample, q, ω, λ)` cells of each point.
fn points(r: &QuantReport, s: &Series) -> Vec<[String; 5]> {
    let n = r.sample_size.max(1);
    s.q.iter()
        .zip(&s.w)
        .zip(&s.rate)
        .enumerate()
        .map(|(i, ((q, w), l))| {
            let t = r.times.get(i / n).copied().unwrap_or(f64::NAN);
            [
                py_float(t),
                (i % n).to_string(),
                py_float(*q),
                py_float(*w),
                py_float(*l),
            ]
        })
        .collect()
}

pub fn models(r: &QuantReport) -> String {
    let mut out = String::new();
    row(&mut out, &["id".into(), "label".into(), "is_used".into()]);
    for m in &r.models {
        row(
            &mut out,
            &[
                m.id.clone(),
                cell(m.label.as_deref()),
                py_bool(m.is_used).into(),
            ],
        );
    }
    out
}

pub fn events(r: &QuantReport) -> String {
    let mut out = String::new();
    let head = [
        "id",
        "label",
        "is_used",
        "time",
        "sample",
        "computed_probability",
        "computed_intensity",
        "computed_rate",
    ];
    row(&mut out, &head.map(String::from));
    for e in &r.events {
        for p in points(r, &e.series) {
            let mut c = vec![
                e.id.clone(),
                cell(e.label.as_deref()),
                py_bool(e.is_used).into(),
            ];
            c.extend(p);
            row(&mut out, &c);
        }
    }
    out
}

pub fn gates(r: &QuantReport) -> String {
    let mut out = String::new();
    let head = [
        "id",
        "label",
        "is_top_gate",
        "is_paged",
        "type",
        "inputs",
        "time",
        "sample",
        "computed_probability",
        "computed_intensity",
        "computed_rate",
    ];
    row(&mut out, &head.map(String::from));
    for g in &r.gates {
        let ty = match g.gate_type {
            super::GateType::Vote(_) => "VOTE".to_string(),
            t => t.name(),
        };
        for p in points(r, &g.series) {
            let mut c = vec![
                g.id.clone(),
                cell(g.label.as_deref()),
                py_bool(g.is_top_gate).into(),
                py_bool(g.is_paged).into(),
                ty.clone(),
                g.inputs.join(","),
            ];
            c.extend(p);
            row(&mut out, &c);
        }
    }
    out
}

/// The cut-set table of gate number `gate` (in file order).
pub fn cut_sets(r: &QuantReport, gate: usize) -> Option<String> {
    let g = r.gates.get(gate)?;
    let mut out = String::new();
    let head = [
        "cut_set",
        "order",
        "time",
        "sample",
        "computed_probability",
        "computed_intensity",
        "computed_rate",
        "probability_importance",
        "intensity_importance",
    ];
    row(&mut out, &head.map(String::from));
    let sum_at = |i: usize, q: bool| -> f64 {
        let mut parts: Vec<f64> = g
            .cut_sets
            .iter()
            .map(|c| {
                let s = if q { &c.series.q } else { &c.series.w };
                s.get(i).copied().unwrap_or(f64::NAN)
            })
            .collect();
        parts.sort_by(|a, b| b.total_cmp(a));
        parts.into_iter().sum()
    };
    for c in &g.cut_sets {
        let name = if c.events.is_empty() {
            "True".to_string()
        } else {
            c.events.join(".")
        };
        for (i, p) in points(r, &c.series).into_iter().enumerate() {
            let q = c.series.q.get(i).copied().unwrap_or(f64::NAN);
            let w = c.series.w.get(i).copied().unwrap_or(f64::NAN);
            let mut cells = vec![name.clone(), c.events.len().to_string()];
            cells.extend(p);
            cells.push(py_float(super::compute::divide(q, sum_at(i, true))));
            cells.push(py_float(super::compute::divide(w, sum_at(i, false))));
            row(&mut out, &cells);
        }
    }
    Some(out)
}

/// The importance table of gate number `gate` (in file order), with PFTA's
/// columns. Empty of rows when importances were not computed.
pub fn importances(r: &QuantReport, gate: usize) -> Option<String> {
    let g = r.gates.get(gate)?;
    let mut out = String::new();
    let head = [
        "event",
        "label",
        "time",
        "sample",
        "marginal_importance",
        "criticality_importance",
        "diagnostic_importance",
        "prognostic_importance",
        "risk_achievement_worth",
        "risk_reduction_worth",
    ];
    row(&mut out, &head.map(String::from));
    let n = r.sample_size.max(1);
    for m in &g.importances {
        for (i, marginal) in m.marginal.iter().enumerate() {
            let at = |v: &[f64]| py_float(v.get(i).copied().unwrap_or(f64::NAN));
            let t = r.times.get(i / n).copied().unwrap_or(f64::NAN);
            row(
                &mut out,
                &[
                    m.event.clone(),
                    cell(m.label.as_deref()),
                    py_float(t),
                    (i % n).to_string(),
                    py_float(*marginal),
                    at(&m.criticality),
                    at(&m.diagnostic),
                    at(&m.prognostic),
                    at(&m.risk_achievement_worth),
                    at(&m.risk_reduction_worth),
                ],
            );
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::py_float;

    #[test]
    fn floats_look_like_python() {
        assert_eq!(py_float(0.1), "0.1");
        assert_eq!(py_float(1.0), "1.0");
        assert_eq!(py_float(0.0), "0.0");
        assert_eq!(py_float(1e-5), "1e-05");
        assert_eq!(py_float(2.5e-7), "2.5e-07");
        assert_eq!(py_float(1000.0), "1000.0");
        assert_eq!(py_float(1e16), "1e+16");
        assert_eq!(py_float(f64::NAN), "nan");
        assert_eq!(py_float(-f64::INFINITY), "-inf");
    }
}
