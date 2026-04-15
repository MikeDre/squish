use anyhow::Result;
use rayon::prelude::*;
use squish_core::{squish_file, Format, SquishError, SquishOptions, SquishResult};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub struct RunConfig {
    pub opts: SquishOptions,
    pub verbose: bool,
    pub quiet: bool,
    pub dry_run: bool,
}

pub struct RunReport {
    pub results: Vec<SquishResult>,
    pub errors: Vec<(PathBuf, SquishError)>,
    pub skipped_unknown: Vec<PathBuf>,
    pub total_wall: Duration,
}

impl RunReport {
    pub fn input_bytes(&self) -> u64 {
        self.results.iter().map(|r| r.input_bytes).sum()
    }
    pub fn output_bytes(&self) -> u64 {
        self.results.iter().map(|r| r.output_bytes).sum()
    }
    pub fn exit_code(&self) -> u8 {
        if self.errors.is_empty() { 0 } else { 1 }
    }
}

pub fn run(paths: &[PathBuf], cfg: &RunConfig) -> Result<RunReport> {
    let start = Instant::now();

    let mut known = Vec::new();
    let mut skipped_unknown = Vec::new();

    for path in paths {
        match peek_format(path) {
            Ok(Some(_)) => known.push(path.clone()),
            Ok(None) => skipped_unknown.push(path.clone()),
            Err(_) => {
                // Let squish_file report the real I/O error.
                known.push(path.clone());
            }
        }
    }

    if cfg.dry_run {
        for p in &known {
            println!("would squish: {}", p.display());
        }
        for p in &skipped_unknown {
            println!("would skip (unrecognized): {}", p.display());
        }
        return Ok(RunReport {
            results: Vec::new(),
            errors: Vec::new(),
            skipped_unknown,
            total_wall: start.elapsed(),
        });
    }

    let processed = AtomicU64::new(0);
    let total = known.len() as u64;

    let pairs: Vec<(PathBuf, Result<SquishResult, SquishError>)> = known
        .par_iter()
        .map(|path| {
            let res = squish_file(path, &cfg.opts);
            let n = processed.fetch_add(1, Ordering::SeqCst) + 1;
            if !cfg.quiet && cfg.verbose {
                match &res {
                    Ok(r) => {
                        eprintln!(
                            "[{n}/{total}] {} → {} ({:.1}% saved)",
                            path.display(),
                            r.output_path.display(),
                            r.reduction_percent()
                        );
                    }
                    Err(e) => {
                        eprintln!("[{n}/{total}] {}: ERROR {e}", path.display());
                    }
                }
            }
            (path.clone(), res)
        })
        .collect();

    let mut results = Vec::new();
    let mut errors = Vec::new();
    for (p, r) in pairs {
        match r {
            Ok(r) => results.push(r),
            Err(e) => errors.push((p, e)),
        }
    }

    let report = RunReport {
        results,
        errors,
        skipped_unknown,
        total_wall: start.elapsed(),
    };

    if !cfg.quiet {
        print_summary(&report);
    }

    Ok(report)
}

fn peek_format(path: &Path) -> std::io::Result<Option<Format>> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut head = [0u8; 32];
    let n = f.read(&mut head)?;
    Ok(squish_core::detect_format(path, &head[..n]))
}

fn print_summary(r: &RunReport) {
    let in_mb = r.input_bytes() as f64 / 1_048_576.0;
    let out_mb = r.output_bytes() as f64 / 1_048_576.0;
    let saved = if r.input_bytes() > 0 {
        (1.0 - r.output_bytes() as f64 / r.input_bytes() as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "Squished {} files · {:.1} MB → {:.1} MB ({:+.1}%) · {}",
        r.results.len(),
        in_mb,
        out_mb,
        -saved,
        humantime::format_duration(trim_sub_ms(r.total_wall))
    );
    if !r.skipped_unknown.is_empty() {
        let names: Vec<String> = r
            .skipped_unknown
            .iter()
            .take(5)
            .map(|p| p.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string())
            .collect();
        let extra = r.skipped_unknown.len().saturating_sub(5);
        let list = if extra > 0 {
            format!("{}, and {extra} more", names.join(", "))
        } else {
            names.join(", ")
        };
        println!("Skipped {} (unrecognized: {list})", r.skipped_unknown.len());
    }
    if !r.errors.is_empty() {
        eprintln!("\nErrors ({}):", r.errors.len());
        for (p, e) in &r.errors {
            eprintln!("  {}: {e}", p.display());
        }
    }
}

fn trim_sub_ms(d: Duration) -> Duration {
    Duration::from_millis(d.as_millis() as u64)
}
