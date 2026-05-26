use std::env;

fn main() -> Result<(), car::CarError> {
    let path = env::args()
        .nth(1)
        .expect("usage: cargo run -p car-parser --example diagnostics -- <file.car>");
    let archive = car::Car::new(path)?;
    let report = archive.diagnostics();

    println!("facets: {}", report.totals.facets);
    println!("entries: {}", report.totals.entries);
    println!("unsupported outputs: {}", report.totals.unsupported_outputs);
    println!(
        "unresolved references: {}",
        report.totals.unresolved_references
    );
    println!("unknown TLVs: {}", report.totals.unknown_tlvs);

    for entry in report
        .entries
        .iter()
        .filter(|entry| !entry.issues.is_empty())
        .take(20)
    {
        println!("{}: {:?}", entry.facet_name, entry.issues);
    }

    Ok(())
}
