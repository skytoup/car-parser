#[cfg(feature = "image")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::env;
    use std::path::PathBuf;

    let mut args = env::args().skip(1);
    let path = args
        .next()
        .expect("usage: cargo run -p car --features image --example export_png -- <file.car> <asset-name> <out.png>");
    let name = args.next().unwrap_or_else(|| "Image/png".to_string());
    let out = args
        .map(PathBuf::from)
        .next()
        .unwrap_or_else(|| "asset.png".into());

    let archive = car::Car::new(path)?;
    let variant = archive.best_variant_for_name(&name, &car::VariantQuery::new())?;
    let item = archive
        .item_with_key_values(&variant.key_values)
        .expect("variant should resolve to source item");

    car::image::save_image(item, &out)?;
    println!("wrote {}", out.display());

    Ok(())
}

#[cfg(not(feature = "image"))]
fn main() {
    eprintln!(
        "enable the `image` feature: cargo run -p car --features image --example export_png -- <file.car>"
    );
}
