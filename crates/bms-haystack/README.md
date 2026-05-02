# bms-haystack

Project Haystack 5 / Xeto **ontology + tagging** for `bms-store`. Intentionally narrow:

- **Ontology** — `TAGS` and `EQUIP_PROTOTYPES` / `POINT_PROTOTYPES` tables.
  `Haystack5Provider` exposes a `TagDef` list derived at build time from the
  vendored upstream xeto bundle (`assets/xeto-master/`, version 5.0.0).
  The hand-curated `Haystack4Provider` is retained for backward compatibility.
- **Auto-tag** — name-pattern + unit heuristics for points, name-pattern for
  equipment. Conservative; meant as a starter that humans then bulk-edit.
- **Version pin** — `xeto::version::VENDORED_PH_VERSION`.

## Out of scope

bms-store is a data layer, not a Haystack server. The following are
**deliberately not shipped** here:

- Wire-format codecs (Hayson / Zinc / Trio / CSV)
- Filter parser / evaluator
- HTTP `/api/haystack/*` facade
- Runtime xeto loader / namespace
- Schema-aware validator

If a downstream consumer ever needs them, they live in git history at
commit `befdb12` and earlier on `main`.

## Layout

```
src/
  ontology/      tag tables, prototype tables, Haystack5Provider
  auto_tag.rs    point/equip tag suggestions
  xeto/version.rs  pinned upstream version
build.rs         walks assets/xeto-master/ → OUT_DIR/generated.rs
tests/
  parity.rs      legacy vs generated coverage diagnostic
```

## Use

```rust
use bms_haystack::ontology::{Haystack5Provider, TagProvider};
use bms_haystack::auto_tag::{suggest_equip_tags, suggest_point_tags};

let provider = Haystack5Provider;
let tags = suggest_point_tags("ahu1-discharge-air-temp", Some("°F"), &Default::default(), &provider);
```

## Tests

```bash
cargo test -p bms-haystack
```
