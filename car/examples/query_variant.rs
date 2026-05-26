use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let path = args
        .next()
        .expect("usage: cargo run -p car --example query_variant -- <file.car> <asset-name>");
    let name = args.next().unwrap_or_else(|| "Image/png".to_string());
    let archive = car::Car::new(path)?;

    let query = car::VariantQuery::new().scale(1);
    let variant = archive.best_variant_for_name(&name, &query)?;

    println!(
        "{} -> {} {:?}",
        variant.facet_name, variant.rendition_name, variant.key_values
    );

    Ok(())
}
