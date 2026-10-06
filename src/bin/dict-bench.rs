use anyhow::{Result, ensure};
use clap::Parser;
use local_english_dict::{App, Dictionary, draw};
use ratatui::{Terminal, backend::TestBackend};
use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(about = "Measure full lookup+preview and asynchronous input-to-render latency")]
struct Args {
    #[arg(long)]
    data: PathBuf,
    #[arg(long, default_value_t = 100)]
    iterations: usize,
}

fn percentile(values: &mut [f64], p: usize) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) * p / 100).min(values.len() - 1)]
}

fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(args.iterations > 0, "iterations must be positive");
    let opened = Instant::now();
    let dictionary = Dictionary::open(&args.data)?;
    println!(
        "Dictionary open: {:.3}ms (OS caches not flushed)",
        opened.elapsed().as_secs_f64() * 1000.
    );
    println!("Candidates: {}", dictionary.candidate_count());
    drop(dictionary);
    for (kind, query) in [
        ("single-letter", "h"),
        ("prefix", "ho"),
        ("exact", "house"),
        ("fuzzy", "hosue"),
        ("form", "went"),
        ("phrase", "take off"),
    ] {
        let mut fresh = Dictionary::open(&args.data)?;
        let first = Instant::now();
        let matches = fresh.search(query)?;
        if let Some(c) = matches.first() {
            fresh.preview(c)?;
        }
        let first_ms = first.elapsed().as_secs_f64() * 1000.;
        let mut values = vec![];
        for _ in 0..args.iterations {
            let start = Instant::now();
            let matches = fresh.search(query)?;
            if let Some(c) = matches.first() {
                fresh.preview(c)?;
            }
            values.push(start.elapsed().as_secs_f64() * 1000.);
        }
        let p50 = percentile(&mut values, 50);
        let p95 = percentile(&mut values, 95);
        println!(
            "{kind:14} {query:10} first-cache-read={first_ms:.3}ms warm P50={p50:.3}ms P95={p95:.3}ms"
        );
    }
    println!("First-cache-read uses a fresh application cache; OS file caches are not flushed.");
    let mut app = App::new(Dictionary::open(&args.data)?, "");
    let mut terminal = Terminal::new(TestBackend::new(120, 40))?;
    let mut values = vec![];
    for i in 0..args.iterations {
        let start = Instant::now();
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('u'),
            crossterm::event::KeyModifiers::CONTROL,
        ));
        app.paste(if i % 2 == 0 { "ho" } else { "hosue" });
        while app.loading {
            app.poll();
            ensure!(start.elapsed() < Duration::from_secs(10), "Worker timeout");
            thread::sleep(Duration::from_millis(1));
        }
        terminal.draw(|f| draw(f, &mut app))?;
        values.push(start.elapsed().as_secs_f64() * 1000.);
    }
    println!(
        "Async input-to-TestBackend-render 120x40 P95={:.3}ms (does not include terminal emulator display)",
        percentile(&mut values, 95)
    );
    Ok(())
}
