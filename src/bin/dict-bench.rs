use anyhow::{Result, ensure};
use clap::Parser;
use ratatui::{Terminal, backend::TestBackend};
use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};
use wordglide::{App, Dictionary, draw};

#[derive(Parser)]
#[command(about = "Measure full lookup+preview and asynchronous input-to-render latency")]
struct Args {
    #[arg(long)]
    data: PathBuf,
    #[arg(long, default_value_t = 100)]
    iterations: usize,
    /// Newline-separated fixed queries; each is measured with fresh application caches.
    #[arg(long)]
    queries: Option<PathBuf>,
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
    if let Some(path) = &args.queries {
        let text = std::fs::read_to_string(path)?;
        let queries: Vec<_> = text.lines().filter(|q| !q.trim().is_empty()).collect();
        ensure!(!queries.is_empty(), "query file is empty");
        let mut opens = Vec::new();
        let mut cold = Vec::new();
        let mut warm = Vec::new();
        for query in &queries {
            let start = Instant::now();
            let mut dictionary = Dictionary::open(&args.data)?;
            opens.push(start.elapsed().as_secs_f64() * 1000.);
            for values in [&mut cold, &mut warm] {
                let start = Instant::now();
                let matches = dictionary.search(query)?;
                ensure!(
                    !matches.is_empty(),
                    "No results for benchmark query: {query}"
                );
                dictionary.preview(&matches[0])?;
                values.push(start.elapsed().as_secs_f64() * 1000.);
            }
        }
        println!(
            "Fixed queries={} open P50={:.3}ms P95={:.3}ms uncached lookup+preview P50={:.3}ms P95={:.3}ms cached P95={:.3}ms (OS caches not flushed)",
            queries.len(),
            percentile(&mut opens, 50),
            percentile(&mut opens, 95),
            percentile(&mut cold, 50),
            percentile(&mut cold, 95),
            percentile(&mut warm, 95),
        );
    }
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
    for query in ["set", "take", "run"] {
        let mut app = App::new(Dictionary::open(&args.data)?, query);
        let deadline = Instant::now();
        while app.loading {
            app.poll();
            ensure!(
                deadline.elapsed() < Duration::from_secs(10),
                "Worker timeout"
            );
            thread::sleep(Duration::from_millis(1));
        }
        if !app.preview.as_ref().is_some_and(|p| p.entry.key == query) {
            continue;
        }
        let key = |code, modifiers| crossterm::event::KeyEvent::new(code, modifiers);
        use crossterm::event::{KeyCode, KeyModifiers};
        app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
        let first = Instant::now();
        terminal.draw(|f| draw(f, &mut app))?;
        let first_draw = first.elapsed().as_secs_f64() * 1000.;
        let mut scroll = Vec::new();
        for _ in 0..args.iterations {
            let start = Instant::now();
            app.handle_key(key(KeyCode::Char('j'), KeyModifiers::CONTROL));
            terminal.draw(|f| draw(f, &mut app))?;
            scroll.push(start.elapsed().as_secs_f64() * 1000.);
        }
        let first = Instant::now();
        app.handle_key(key(KeyCode::Char('/'), KeyModifiers::NONE));
        app.paste("the");
        terminal.draw(|f| draw(f, &mut app))?;
        let find_first = first.elapsed().as_secs_f64() * 1000.;
        app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
        let mut find = Vec::new();
        let mut reflow = Vec::new();
        for i in 0..args.iterations {
            let start = Instant::now();
            app.handle_key(key(KeyCode::Char('n'), KeyModifiers::NONE));
            terminal.draw(|f| draw(f, &mut app))?;
            find.push(start.elapsed().as_secs_f64() * 1000.);
            let mut resized =
                Terminal::new(TestBackend::new(if i % 2 == 0 { 80 } else { 120 }, 40))?;
            let start = Instant::now();
            app.handle_key(key(KeyCode::Char('e'), KeyModifiers::NONE));
            resized.draw(|f| draw(f, &mut app))?;
            reflow.push(start.elapsed().as_secs_f64() * 1000.);
        }
        println!(
            "Reading {query:5} first-draw={first_draw:.3}ms find-first={find_first:.3}ms scroll P95={:.3}ms next-match P95={:.3}ms resize+examples P95={:.3}ms",
            percentile(&mut scroll, 95),
            percentile(&mut find, 95),
            percentile(&mut reflow, 95)
        );
    }
    Ok(())
}
