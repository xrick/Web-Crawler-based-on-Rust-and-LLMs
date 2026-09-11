# Repository Guidelines

## Project Structure & Module Organization

This Rust 2024 application crawls Apple Taiwan product specifications and uses local Ollama models for classification.

- `src/main.rs`: Actix server startup, shared pages, and the `/todos` example.
- `src/api.rs` and `src/crawler.rs`: HTTP endpoints and background job orchestration.
- `src/network.rs`, `src/apple.rs`, and `src/llm.rs`: fetch policies, specification extraction, and model integration.
- `src/models.rs` and `src/storage.rs`: shared data types and SQLite persistence.
- `src/*.html` and `src/app.js`: embedded browser assets; rebuild and restart after edits.
- `src/tests.rs` and `tests/fixtures/`: automated tests and HTML fixtures.
- `scripts/verify_live.py`: live verification; writes snapshots to `output/verification/`.

## Build, Test, and Development Commands

Run commands from the repository root using a Rust toolchain supporting edition 2024.

- `cargo build`: compile the application.
- `cargo run`: start the server at `http://127.0.0.1:8080`.
- `ollama serve` and `ollama pull qwen2.5-coder:14b`: start Ollama and install the default model; settings allow another installed model.
- `cargo test`: run automated tests.
- `cargo fmt --check`: check Rust formatting; use `cargo fmt` to apply it.
- `cargo clippy --all-targets -- -D warnings`: lint all targets, treating warnings as errors.
- `python3 scripts/verify_live.py --mode sample`: verify one product per selected category with the running server, Apple network access, and Ollama. Use `discovery` for link discovery or `full` for all products.

## Coding Style & Naming Conventions

Use rustfmt defaults and four-space Rust indentation. Name functions/modules in `snake_case`, types in `PascalCase`, and constants in `SCREAMING_SNAKE_CASE`. Follow existing two-space JavaScript indentation and camelCase naming. Keep mutex guards out of network `.await` calls, and render fetched text with `textContent`.

## Testing Guidelines

Tests use Cargo's test harness and `#[actix_web::test]`. Add descriptive snake_case cases in `src/tests.rs` and deterministic HTML fixtures under `tests/fixtures/`. Cover extraction, source preservation, URL boundaries, validation, persistence, and cancellation when affected. No numeric coverage threshold is configured. Run formatting, linting, and tests before submitting changes.

## Commit & Pull Request Guidelines

History contains only `Initial commit` and `add`; no formal convention is established. Use concise imperative subjects, such as `Fix model grouping for shared table cells`. Keep changes focused. PRs should explain behavior changes, link relevant issues, report validation commands/results, and include screenshots for UI changes.

## Configuration & Data Handling

Set `CRAWLER_DATA_DIR` to isolate runtime data; the default is `data/`, containing SQLite and `runs/{job_id}/` artifacts. Keep generated data and `target/` out of new commits. Preserve localhost access controls, Apple Taiwan URL restrictions, robots handling, and source-backed specification values.
