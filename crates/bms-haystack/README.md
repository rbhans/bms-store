# bms-haystack

Project Haystack 5 / Xeto support for `bms-store`. Provides:

- **Ontology** — build-time-generated tag and spec tables from the vendored
  upstream xeto bundle at `assets/xeto-master/` (Haystack 5.0.0). The
  legacy hand-curated `Haystack4Provider` is kept alongside the new
  `Haystack5Provider` for backward compatibility.
- **Value types** — `Marker`, `NA`, `Remove`, `Bool`, `Number{val,unit}`,
  `Str`, `Uri`, `Ref`, `Symbol`, `XStr`, `Date`, `Time`, `DateTime`, `Coord`,
  `List`, `Dict`, `Grid`.
- **Codecs** — Hayson (JSON) round-trip. Zinc/Trio/CSV slots reserved.
- **Filter expressions** — recursive-descent parser, in-memory evaluator
  with arrow-path Ref walks, plus a SQL push-down lowerer for SQLite.
- **HTTP facade** (`server` feature) — axum router for the standard
  Haystack op set: `about`, `defs`, `libs`, `ops`, `filetypes`, `read`,
  `nav`, `watchSub/Unsub/Poll`, `pointWrite`, `hisRead`, `hisWrite`,
  `invokeAction`. Content-negotiates Hayson by default.
- **Runtime xeto loader** — load custom `.xeto` libraries from disk into a
  swappable namespace; merges with the build-time generated tables.
- **Schema-aware validator** — checks Dict tag kinds and quantities
  against the loaded namespace.

## Layout

```
src/
  ontology/      tag/proto/provider tables; codegen-backed
  val/           Value enum and friends
  codec/hayson/  JSON encode/decode
  filter/        AST + parser + eval + SQL lowerer
  server/        axum router and handlers (feature: server)
  validation/    schema-aware Dict checks
  xeto/          parser, loader, namespace, version pin
  auto_tag.rs    name-pattern → tag heuristics
build.rs         walks assets/xeto-master/ → OUT_DIR/generated.rs
tests/
  parity.rs      legacy vs generated coverage diagnostic
  http_smoke.rs  HTTP integration tests (server feature)
```

## Quick start

In a downstream crate:

```rust
use std::sync::Arc;
use bms_haystack::server::{router, HaystackState};

let state: Arc<dyn HaystackState> = build_my_state();
let app = axum::Router::new().nest("/api/haystack", router(state));
```

`bms-store-server` already does this in
[`api/routes/mod.rs`](../bms-store-server/src/api/routes/mod.rs); see
[`api/haystack_state.rs`](../bms-store-server/src/api/haystack_state.rs)
for the storage-backed `HaystackState` impl.

## Try it

After `cargo build`, hit the demo server:

```bash
cargo run --bin bms-stored -- --data-dir demo-data &
curl -s http://localhost:8080/api/haystack/about | jq
curl -s 'http://localhost:8080/api/haystack/read?filter=ahu' | jq
curl -s 'http://localhost:8080/api/haystack/read?filter=point%20and%20discharge' | jq
curl -s http://localhost:8080/api/haystack/defs | jq '.rows | length'
```

## Custom xeto libraries

Drop a custom library into `data/xeto/<libname>/lib.xeto` and point the
loader at `data/xeto/`:

```rust
use bms_haystack::xeto::{HaystackNamespace, SharedNamespace};

let ns = HaystackNamespace::load(std::path::Path::new("data/xeto"))?;
let shared = std::sync::Arc::new(SharedNamespace::new(ns));
// Reload on SIGHUP:
let snapshot = shared.current();
shared.swap_in(snapshot.reload()?);
```

## Versioning

The vendored Haystack version is pinned at
[`xeto::version::VENDORED_PH_VERSION`](src/xeto/version.rs). Refresh by
swapping `assets/xeto-master/` and bumping the constant.

## Tests

```bash
cargo test -p bms-haystack                           # 78 unit tests
cargo test -p bms-haystack --test parity             # 5 parity diagnostics
cargo test -p bms-haystack --features server --test http_smoke   # 6 HTTP smoke tests
```
