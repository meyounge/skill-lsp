//! Reproducible local microbenchmark, not a performance claim.
//! cargo run --release --example perf
use skill_lsp::{
    compat::Document,
    fnd::{self, Database},
};
use std::{hint::black_box, path::Path, time::Instant};
fn measure(name: &str, n: usize, mut f: impl FnMut()) {
    let start = Instant::now();
    for _ in 0..n {
        f();
    }
    println!(
        "{name}: {:.2} us/op ({n} iterations)",
        start.elapsed().as_secs_f64() * 1e6 / n as f64
    );
}
fn main() {
    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "examples/SKILL".into());
    let entries = fnd::load_dir(Path::new(&root)).expect("local .fnd directory");
    let db = Database::new(entries);
    println!("{} functions", db.entries.len());
    let source = "procedure(testFn(cell)\n dbSynthetic(cell \"name\" ?width 2)\n)\n".repeat(1000);
    let d = Document::new(source.clone(), &db);
    measure("parse 3000 lines", 50, || {
        black_box(Document::new(source.clone(), &db));
    });
    measure("exact lookup", 100000, || {
        black_box(db.get(black_box("dbSynthetic")));
    });
    measure("prefix lookup (first 300)", 1000, || {
        black_box(db.prefix(black_box("db")).take(300).count());
    });
    measure("cached semantic token clone", 100, || {
        black_box(d.semantic.clone());
    });
}
