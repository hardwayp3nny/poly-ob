use anyhow::Result;
use clap::Parser;
use csv::StringRecord;
use std::collections::{HashMap, HashSet};

#[derive(Parser, Debug)]
struct Args {
    /// Input CSV path
    #[arg(long, default_value = "bench.csv")]
    input: String,
}

fn percentile(sorted: &Vec<i128>, p: f64) -> i128 {
    if sorted.is_empty() { return 0; }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx]
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut rdr = csv::Reader::from_path(&args.input)?;

    let mut ws: HashMap<String, i128> = HashMap::new();
    let mut rest: HashMap<String, i128> = HashMap::new();

    for rec in rdr.records() {
        let rec: StringRecord = rec?;
        if rec.len() < 3 { continue; }
        let source = rec.get(0).unwrap().trim();
        let hash = rec.get(1).unwrap().trim().to_string();
        let ts: i128 = rec.get(2).unwrap().trim().parse().unwrap_or(0);
        match source {
            "ws" => { ws.entry(hash).or_insert(ts); },
            "rest" => { rest.entry(hash).or_insert(ts); },
            _ => {}
        }
    }

    let ws_set: HashSet<_> = ws.keys().cloned().collect();
    let rest_set: HashSet<_> = rest.keys().cloned().collect();
    let common: HashSet<_> = ws_set.intersection(&rest_set).cloned().collect();

    println!("ws unique hashes: {}", ws_set.len());
    println!("rest unique hashes: {}", rest_set.len());
    println!("common hashes: {}", common.len());

    // diffs (ws_ts - rest_ts) for common
    let mut diffs: Vec<i128> = Vec::with_capacity(common.len());
    for h in &common {
        if let (Some(&tw), Some(&tr)) = (ws.get(h), rest.get(h)) {
            diffs.push(tw - tr);
        }
    }
    diffs.sort();
    if diffs.is_empty() {
        println!("no common diffs");
        return Ok(());
    }

    let sum: i128 = diffs.iter().copied().sum();
    let avg = (sum as f64) / (diffs.len() as f64);
    let median = percentile(&diffs, 0.5);
    let p25 = percentile(&diffs, 0.25);
    let p75 = percentile(&diffs, 0.75);
    let min = diffs.first().copied().unwrap();
    let max = diffs.last().copied().unwrap();

    println!("diff (ws-rest) stats (ms):");
    println!("  avg   : {:.2}", avg);
    println!("  median: {}", median);
    println!("  p25   : {}", p25);
    println!("  p75   : {}", p75);
    println!("  min   : {}", min);
    println!("  max   : {}", max);

    Ok(())
}


