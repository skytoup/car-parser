use std::env;

fn main() -> Result<(), car::CarError> {
    let path = env::args()
        .nth(1)
        .expect("usage: cargo run -p car-parser --example list_entries -- <file.car>");
    let archive = car::Car::new(path)?;

    for entry in archive.entries() {
        println!(
            "{} {:?} {:?} {}x{} scale {}",
            entry.facet_name,
            entry.kind,
            entry.payload_kind,
            entry.width,
            entry.height,
            entry.scale
        );
    }

    Ok(())
}
