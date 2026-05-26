# CarParser

[English](README.en.md)

CarParser 是一个用于解析 Apple `.car` asset catalog 文件的 Rust workspace，包含高层 Rust API、命令行工具、WASM 包装层和浏览器本地解码界面。

项目当前重点是读取 CoreUI `.car` 归档、列出资源、查询变体、解析元数据、导出可用资源，并在浏览器内完成本地预览和下载。浏览器界面只读取本地文件，不上传服务器。

在线体验：https://car.skytoup.com/

## 功能

- 解析 Apple BOM 和 `.car` 内部 rendition 数据。
- 提供 `car` crate 作为稳定高层 API。
- 支持按 Facet、Rendition、scale、idiom、display gamut 等属性查询资源变体。
- 支持诊断报告，统计 unsupported output、internal reference、unknown TLV 等问题。
- 支持导出 PNG、JPEG、WEBP、HEIF、PDF、SVG、原始数据和颜色 JSON。
- 支持解码常见 raster payload，包括 ARGB、ARGB16、GRAY、GA8、GA16、RGB5、JPEG、WEBP，以及 Deepmap2 相关 payload。
- 提供 `car-cli` 命令用于查看归档信息和批量导出资源。
- 提供 `car-wasm` 和 `car-web-decoder-site`，用于浏览器内解析、预览、搜索、单项下载和 ZIP 批量下载。

## 目录

| 路径 | 说明 |
| --- | --- |
| `bom/` | Apple BOM 读取和底层模型。 |
| `deepmap2/` | Deepmap2 解码相关实现。 |
| `util/` | 压缩和共享工具。 |
| `car/` | 主库 crate。公开 API 在 `car/src/lib.rs`。 |
| `car-cli/` | `info` 和 `extract` 命令。 |
| `car-wasm/` | wasm-bindgen 包装层和 JS worker/client bridge。 |
| `car-web-decoder-site/` | Vite、React、TypeScript、Tailwind 浏览器解码站点。 |
| `car-tests/` | Rust 集成测试和测试 fixture。 |
| `test-support/` | 测试辅助 crate。 |

## 环境

- Rust toolchain，项目使用 Rust edition 2024。
- Node.js 和 pnpm，用于 `car-web-decoder-site/`。
- `wasm-pack`，用于构建 `car-wasm`。Web 项目的 `predev`、`prebuild`、`pretest*` 会自动调用 `pnpm refresh:wasm`，该脚本依赖 PATH 中存在 `wasm-pack`。

## 快速开始

构建默认 workspace members，当前为 `bom` 和 `car`：

```bash
cargo build
```

运行默认测试：

```bash
cargo test
```

运行指定 crate 测试：

```bash
cargo test -p car
cargo test -p car-cli
```

查看 `.car` 信息：

```bash
cargo run -p car-cli -- info <file.car>
```

导出 `.car` 资源：

```bash
cargo run -p car-cli -- extract <file.car> -o <out-dir>
cargo run -p car-cli -- extract <file.car> -o <out-dir> --overwrite
```

`info` 输出 JSON，结构接近 `assetutil -I`。`extract` 会创建输出目录，默认跳过已有文件，使用 `--overwrite` 覆盖已有文件。

## Rust API

`car` crate 是主要入口。默认构建不启用图片写出 helper；需要 `car::image` 时启用 `image` feature。

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

图片导出 helper：

```bash
cargo run -p car --features image --example export_png -- <file.car> <asset-name> <out.png>
```

更多示例在 `car/examples/`：

- `list_entries.rs`：列出高层资源条目。
- `query_variant.rs`：按属性查询最佳变体。
- `export_plan.rs`：查看确定性导出计划。
- `export_png.rs`：启用 `image` feature 后导出 PNG。
- `export_original.rs`：导出原始 payload。
- `diagnostics.rs`：输出诊断摘要。

## CLI

`car-cli` 暴露两个子命令：

```bash
cargo run -p car-cli -- info <file.car>
cargo run -p car-cli -- extract <file.car> -o <out-dir> [--overwrite]
```

导出规则由 `car::export::plan_export` 统一规划：

- 图片类 ThemeCBCK payload 会按可解码格式保存，必要时转成 PNG。
- JPEG、WEBP、SVG、PDF、HEIF 和 `Data` payload 尽量保留原始格式。
- Color rendition 导出为 JSON，包含 color space 和 components。
- Multisize image set 导出为 JSON。
- InternalReference 会解析到实际 payload，并应用 crop 信息。
- 输出路径会清理不安全路径片段，并为重名文件生成唯一后缀。

## Web Decoder

浏览器解码站点在 `car-web-decoder-site/`：

```bash
cd car-web-decoder-site
pnpm install
pnpm dev
```

生产构建和测试：

```bash
pnpm build
pnpm typecheck
pnpm test:unit
pnpm test:e2e
```

站点功能：

- 拖放或选择单个 `.car` 文件。
- 展示 document metadata、资源列表、搜索、详情和高级参数。
- 支持 `img-binary`、`canvas-rgba`、`color-swatch` 预览。
- 对 PDF、HEIF、RawData 等资源提供下载路径。
- 支持下载当前资源和 ZIP 批量下载。
- 支持中英文和亮色、暗色、跟随系统主题。

## WASM

`car-wasm` 提供 `WasmArchive` 和浏览器 worker client。Web 站点通过 alias `@car-wasm/client` 指向 `car-wasm/js/client.js`。

可用能力包括：

- `documentInfo()`
- `listEntries()` / `listEntrySummaries()`
- `listImages()` / `listImageSummaries()`
- `getEntryInfo(id)` / `getImageInfo(id)`
- `getDisplayPayload(id)`
- `getDownloadPayload(id)`
- `getThumbnailPayload(id, { maxDimension })`

刷新 WASM 包：

```bash
cd car-web-decoder-site
pnpm refresh:wasm
```

该命令会在 `car-wasm/` 下执行：

```bash
wasm-pack build --target bundler --out-dir pkg --no-opt
```

## 测试

Rust：

```bash
cargo test
cargo test -p car
cargo test -p car-cli
```

Web：

```bash
cd car-web-decoder-site
pnpm test:unit
pnpm test:e2e
```

部分完整 fixture 测试需要额外样本。启用方式：

```bash
CAR_TEST_FULL=1 cargo test
```
