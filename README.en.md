# CarParser

[中文](README.md)

CarParser is a Rust workspace for parsing Apple `.car` asset catalog files. It includes a high-level Rust API, command-line tools, a WASM wrapper, and a local browser decoder UI.

The current focus is reading CoreUI `.car` archives, listing assets, querying variants, parsing metadata, exporting usable resources, and supporting local preview and downloads in the browser. The browser UI only reads local files and does not upload them to a server.

Online demo: https://car.skytoup.com/

## Features

- Parse Apple BOM and internal `.car` rendition data.
- Provide the `car` crate as the stable high-level API.
- Query resource variants by Facet, Rendition, scale, idiom, display gamut, and other attributes.
- Provide diagnostic reports for unsupported output, internal reference, unknown TLV, and related issues.
- Export PNG, JPEG, WEBP, HEIF, PDF, SVG, raw data, and color JSON.
- Decode common raster payloads, including ARGB, ARGB16, GRAY, GA8, GA16, RGB5, JPEG, WEBP, and Deepmap2-related payloads.
- Provide `car-cli` commands for archive inspection and batch resource export.
- Provide `car-wasm` and `car-web-decoder-site` for in-browser parsing, preview, search, single-item download, and ZIP batch download.

## Project Layout

| Path | Description |
| --- | --- |
| `bom/` | Apple BOM reading and low-level models. |
| `deepmap2/` | Deepmap2 decoding implementation. |
| `util/` | Compression and shared utilities. |
| `car/` | Main library crate. Public API lives in `car/src/lib.rs`. |
| `car-cli/` | `info` and `extract` commands. |
| `car-wasm/` | wasm-bindgen wrapper and JS worker/client bridge. |
| `car-web-decoder-site/` | Vite, React, TypeScript, and Tailwind browser decoder site. |
| `car-tests/` | Rust integration tests and test fixtures. |
| `test-support/` | Test helper crate. |

## Requirements

- Rust toolchain. The project uses Rust edition 2024.
- Node.js and pnpm for `car-web-decoder-site/`.
- `wasm-pack` for building `car-wasm`. The web project's `predev`, `prebuild`, and `pretest*` scripts automatically call `pnpm refresh:wasm`, which requires `wasm-pack` to be available in `PATH`.

## Quick Start

Build the default workspace members, currently `bom` and `car`:

```bash
cargo build
```

Run the default tests:

```bash
cargo test
```

Run tests for a specific crate:

```bash
cargo test -p car
cargo test -p car-cli
```

Inspect a `.car` file:

```bash
cargo run -p car-cli -- info <file.car>
```

Extract resources from a `.car` file:

```bash
cargo run -p car-cli -- extract <file.car> -o <out-dir>
cargo run -p car-cli -- extract <file.car> -o <out-dir> --overwrite
```

`info` outputs JSON with a structure close to `assetutil -I`. `extract` creates the output directory, skips existing files by default, and overwrites existing files when `--overwrite` is passed.

## Rust API

The `car` crate is the main entry point. Image writing helpers are not enabled by default. Enable the `image` feature when using `car::image`.

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let archive = car::Car::new("Assets.car")?;

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

    let query = car::VariantQuery::new().scale(1).display_gamut(1);
    let variant = archive.best_variant_for_name("Image/png", &query)?;
    println!("selected {:?} for {}", variant.key_values, variant.facet_name);

    let plan = car::export::plan_export(&archive, "out");
    for job in &plan.jobs {
        println!("{:?} {} -> {}", job.format, job.logical_facet_name, job.path.display());
    }

    let diagnostics = archive.diagnostics();
    println!(
        "{} entries, {} unsupported outputs",
        diagnostics.totals.entries,
        diagnostics.totals.unsupported_outputs
    );

    Ok(())
}
```

Image export helper:

```bash
cargo run -p car --features image --example export_png -- <file.car> <asset-name> <out.png>
```

More examples are available in `car/examples/`:

- `list_entries.rs`: list high-level resource entries.
- `query_variant.rs`: query the best variant by attributes.
- `export_plan.rs`: inspect the deterministic export plan.
- `export_png.rs`: export PNG after enabling the `image` feature.
- `export_original.rs`: export the original payload.
- `diagnostics.rs`: output a diagnostics summary.

## CLI

`car-cli` exposes two subcommands:

```bash
cargo run -p car-cli -- info <file.car>
cargo run -p car-cli -- extract <file.car> -o <out-dir> [--overwrite]
```

Export rules are planned by `car::export::plan_export`:

- Image-like ThemeCBCK payloads are saved in a decodable format and converted to PNG when needed.
- JPEG, WEBP, SVG, PDF, HEIF, and `Data` payloads preserve their original format when possible.
- Color renditions are exported as JSON containing color space and components.
- Multisize image sets are exported as JSON.
- InternalReference entries resolve to the actual payload and apply crop information.
- Output paths strip unsafe path fragments and generate unique suffixes for duplicate filenames.

## Web Decoder

The browser decoder site lives in `car-web-decoder-site/`:

```bash
cd car-web-decoder-site
pnpm install
pnpm dev
```

Production build and tests:

```bash
pnpm build
pnpm typecheck
pnpm test:unit
pnpm test:e2e
```

Site capabilities:

- Drag and drop or select a single `.car` file.
- Display document metadata, resource lists, search, details, and advanced parameters.
- Preview `img-binary`, `canvas-rgba`, and `color-swatch` payloads.
- Provide download paths for PDF, HEIF, RawData, and similar resources.
- Download the current resource or export a ZIP batch.
- Support Chinese, English, light theme, dark theme, and system theme.

## WASM

`car-wasm` provides `WasmArchive` and the browser worker client. The web site maps `@car-wasm/client` to `car-wasm/js/client.js` through an alias.

Available capabilities include:

- `documentInfo()`
- `listEntries()` / `listEntrySummaries()`
- `listImages()` / `listImageSummaries()`
- `getEntryInfo(id)` / `getImageInfo(id)`
- `getDisplayPayload(id)`
- `getDownloadPayload(id)`
- `getThumbnailPayload(id, { maxDimension })`

Refresh the WASM package:

```bash
cd car-web-decoder-site
pnpm refresh:wasm
```

This command runs the following under `car-wasm/`:

```bash
wasm-pack build --target bundler --out-dir pkg --no-opt
```

## Testing

Rust:

```bash
cargo test
cargo test -p car
cargo test -p car-cli
```

Web:

```bash
cd car-web-decoder-site
pnpm test:unit
pnpm test:e2e
```

Some full fixture tests require extra samples. Enable them with:

```bash
CAR_TEST_FULL=1 cargo test
```
