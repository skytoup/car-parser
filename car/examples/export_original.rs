use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let path = args.next().expect(
        "usage: cargo run -p car --example export_original -- <file.car> <asset-name> <out-file>",
    );
    let name = args.next().unwrap_or_else(|| "Image/pdf".to_string());
    let out = args
        .map(PathBuf::from)
        .next()
        .unwrap_or_else(|| "asset.bin".into());

    let archive = car::Car::new(path)?;
    let variant = archive.best_variant_for_name(&name, &car::VariantQuery::new())?;
    let item = archive
        .item_with_key_values(&variant.key_values)
        .expect("variant should resolve to source item");
    let bytes = archive.resolved_source_bytes(item)?;

    fs::write(&out, bytes)?;
    println!("wrote {}", out.display());

    Ok(())
}
