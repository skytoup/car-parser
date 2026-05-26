use std::env;
use std::path::PathBuf;

fn main() -> Result<(), car::CarError> {
    let mut args = env::args().skip(1);
    let path = args
        .next()
        .expect("usage: cargo run -p car-parser --example export_plan -- <file.car> [out-dir]");
    let out = args
        .map(PathBuf::from)
        .next()
        .unwrap_or_else(|| "out".into());
    let archive = car::Car::new(path)?;
    let plan = car::export::plan_export(&archive, out);

    for job in &plan.jobs {
        println!(
            "{:?} {} -> {}",
            job.format,
            job.logical_facet_name,
            job.path.display()
        );
    }

    eprintln!(
        "{} jobs, {} skipped, {} canonical coalesced",
        plan.jobs.len(),
        plan.skipped.len(),
        plan.canonical_coalesced
    );

    Ok(())
}
