# Repository Guidelines

## Project Structure & Module Organization

Rust workspace for parsing Apple `.car` asset catalogs, with a browser decoder UI.

- `bom/`, `deepmap2/`, `util/`: low-level parsing and shared utilities.
- `car/`: main library crate. Public API is in `car/src/lib.rs`; examples live in `car/examples/`.
- `car-cli/`: command-line entry points for `info` and `extract`.
- `car-wasm/`: wasm-bindgen wrapper and JS worker/client bridge.
- `car-tests/`, `test-support/`: integration tests and shared test helpers.
- `car-web-decoder-site/`: Vite, React, TypeScript, Tailwind app with assets in `brand/` and `public/`.

## Build, Test, and Development Commands

Repository root:

- `cargo build`: builds the default workspace members, `car-parser-bom`, `car-parser-deepmap2`, and `car-parser`.
- `cargo test`: runs tests for default members.
- `cargo test -p car-parser` or `cargo test -p car-cli`: runs crate-specific tests.
- `cargo run -p car-cli -- info <file.car>`: prints archive information.
- `cargo run -p car-cli -- extract <file.car> -o <dir>`: extracts archive contents.

`car-web-decoder-site/`:

- `pnpm install`: installs web dependencies.
- `pnpm dev`: refreshes wasm, then starts Vite.
- `pnpm build`: runs TypeScript checks and creates `dist/`.
- `pnpm test:unit`: runs Vitest tests.
- `pnpm test:e2e`: builds the site and runs Playwright tests.

The web `pre*` scripts call `pnpm refresh:wasm`, which runs `wasm-pack build`.

## Coding Style & Naming Conventions

Rust uses edition 2024. Format with `cargo fmt`; keep modules snake_case and public types PascalCase.

TypeScript uses `strict` mode and the `@/*` alias for `src/*`. React components use PascalCase filenames. Hooks and utilities use camelCase or kebab-case matching nearby files.

## Testing Guidelines

Place Rust integration tests under `car-tests/tests/` or beside the code they cover. Frontend unit tests match `src/**/*.test.{ts,tsx}`; E2E specs live in `src/test/e2e/`. Add regression tests for binary decoding, image conversion, export naming, wasm, and UI archive flows.

## Security & Configuration Tips

Do not commit local settings, secrets, generated build output, `target/`, `dist/`, coverage reports, Playwright reports, or test output directories. Keep sample `.car` files and large fixtures out of the repository unless they are required for regression coverage.
