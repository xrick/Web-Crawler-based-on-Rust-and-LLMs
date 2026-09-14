# Development Log

## 2026-09-14 — Current project snapshot

### Repository state

- Branch: `main`.
- HEAD: `52709482ba5bd08ca953cfcefbafab77725ac70e` (`add`).
- Working tree was clean before this log was created.
- Package: `rust-web-starter` version `0.1.0`, Rust edition 2024.
- This update records the existing implementation; no application code was changed.

### Implemented functionality

- Local Apple Taiwan specification crawler for iPhone, Mac, iPad, Apple Watch, and AirPods.
- Actix Web dashboard and settings page with model selection, category selection, page limits, job progress/history, cancellation, and JSON export.
- Product and specification link discovery, HTML/text preservation, SHA-256 page records, and extraction of tables, nested sections, shared columns, qualifiers, and footnotes.
- Local Ollama classification with validated structured output, batching, retries, and source-preserving fallback. Specification values and evidence come from extracted source text rather than model-generated values.
- Official Product JSON-LD TWD starting prices, including named model prices and purchase-page fallback.
- SQLite settings and job snapshots, per-job artifacts, and restart recovery that marks unfinished jobs as `interrupted`.
- Apple URL boundaries, robots rules, request delays/timeouts, cancellation, and localhost mutation checks.
- Discovery, sample, and full live-verification modes with saved snapshots and quality reports.
- Separate in-memory `/todos` example remains available.

### Latest committed work

Commit `5270948` updates price extraction, data models, pipeline handling, and dashboard display to preserve and show prices for individual models. It also includes modern purchase-link handling, regression fixtures/tests, and README updates. Relevant files include `src/apple.rs`, `src/models.rs`, `src/crawler.rs`, `src/crawlers.rs`, and `src/app.js`.

### Code organization

| Location | Responsibility |
| --- | --- |
| `src/main.rs` | Server startup, embedded pages, to-do example |
| `src/api.rs` | Settings, models, jobs, cancellation, export endpoints |
| `src/crawler.rs` | Background job orchestration, budgets, extraction, persistence |
| `src/crawlers.rs`, `src/services.rs` | Crawler traits and injectable download/model services |
| `src/apple.rs`, `src/network.rs`, `src/llm.rs` | Parsing, download policy, Ollama integration |
| `src/models.rs`, `src/storage.rs` | Shared data types and SQLite storage |
| `src/*.html`, `src/app.js` | Embedded browser UI; rebuild/restart after edits |
| `src/tests.rs`, `src/integration_tests.rs`, `tests/fixtures/` | API, parsing, persistence, pipeline regression coverage |
| `scripts/verify_live.py`, `scripts/test_verify_live.py` | Live quality verification and verifier unit tests |
| `.github/workflows/ci.yml` | Formatting, Clippy, Rust and Python checks on push/PR |

### Validation performed for this snapshot

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked` | 26 passed; 4 failed because the sandbox denied loopback socket creation |
| `python3 -m unittest discover -s scripts -p 'test_*.py'` | All 5 tests passed |

The four Rust failures were `network::tests::{cancellation_interrupts_retry_wait,timeouts_exhaust_three_attempts,redirected_host_is_rejected_before_following,transient_errors_retry_but_permanent_errors_do_not}`. Each failed at `src/network.rs:198` with `PermissionDenied: Operation not permitted` during fixture-server setup. Their network behavior remains unverified in this session; rerun `cargo test --locked` in an environment permitting local sockets.

No live Apple/Ollama crawl or browser verification was performed for this snapshot. Existing runtime data and historical reports were not audited.

### Runtime and continuation notes

- Start Ollama with `ollama serve`; install the default model with `ollama pull qwen2.5-coder:14b` if needed.
- Start the application with `cargo run --locked`, then open `http://127.0.0.1:8080`.
- `CRAWLER_PORT` overrides the server port. `CRAWLER_DATA_DIR` overrides the default `crawler_data/apple` data root; use separate roots for separate server instances.
- Ollama currently uses `http://127.0.0.1:11434`. Settings/model-list API handlers call the Ollama integration directly.
- After configuring categories and a model, run `python3 scripts/verify_live.py --mode sample` for a live acceptance check. Use `discovery` for link-only verification or `full` for all candidates within the page budget.
- Only one job runs per Engine process. There is no authentication, scheduling, automatic resume, or cross-process job coordination.
- Cache reuse is limited to a single job. SQLite access is synchronous, and history queries load full job snapshots.
- Apple layout changes, blocked requests, or JavaScript-dependent content may require parser adjustments. Old jobs do not automatically gain newly supported price data; create a new job.
- Preserve original source evidence and review `needs_review` results. `source_matched` indicates source preservation, not human validation.
- Generated runtime data and `target/` remain excluded from commits. See `README.md` for detailed workflows, API contracts, and artifact layout.
