# rust-uasset

Reads Unreal Engine editor packages (`.uasset` / `.umap`, UE 4.21 – 5.8) and prints them as compact,
LLM friendly text (or JSON).

## Usage

```
cargo build --release
target/release/uasset [OPTIONS] <PATH>...
```

`PATH` is a file or a directory (searched recursively for `.uasset` / `.umap`).

| Option | Meaning |
|---|---|
| `-o, --output <FILE>` | write to a file instead of stdout |
| `--json` | JSON output (JSON lines when several files are given) |
| `-v, --verbose` | everything: pin types, node positions, registry tags, undecoded data, metadata |
| `--summary` | header, asset registry tags and dependencies only |
| `--imports` / `--exports` | print the import / export tables |
| `--no-other` | hide objects that are not part of the main view |
| `--max-string <N>` | shorten long strings (default 2000, 0 = unlimited) |

## What is shown

- **Blueprints** (actor, widget, editor utility, ...): parent class, interfaces, variables with types,
  defaults and flags, component tree (SCS) and inherited component overrides, widget tree with slot
  settings, class defaults, and every graph (event graph, functions with signatures, macros,
  collapsed graphs). Graph nodes are numbered in execution order:

  ```
  ### Function IsActionForBlueprints() -> (ReturnValue: bool)  [BlueprintPure, Const]
    N1 Entry AssetActionUtility::IsActionForBlueprints
      -> N2
    N2 Return
      ReturnValue = true
  ```
  Exec links are listed on the source pin (`-> N2`), data links on the consuming pin
  (`Target <- N3.ReturnValue (Get Foo)`), unconnected inputs with their default values.
- **Materials / material functions**: settings, connected outputs and the expression graph
  (`BaseColor <- E1.RGB (VectorParameter "Color")`), comment boxes and the expressions they contain.
- **Material instances**: parent, scalar / vector / texture / static switch parameters, overrides.
- **DataTables**: row struct and one line (or block) per row.
- **DataAssets and any other asset**: all tagged properties of the main object.
- **User defined enums / structs**: values with display names, fields with types and defaults.

## Library

```rust
let asset = rust_uasset::Asset::open(std::path::Path::new("BP_Foo.uasset"))?;
println!("{}", asset.to_text(&rust_uasset::render::Options::default()));
```

`asset.package` holds the summary, names, imports and exports; `asset.objects` the decoded
properties and native data (DataTable rows, graph pins, struct fields, ...) of every export.

## Limitations

- Cooked packages with unversioned properties cannot be decoded (no type information in the file).
- Bulk data (texture/mesh/audio payloads) and Kismet bytecode are skipped.
- Package-level compression and big-endian packages are not supported.

## Tests

`cargo test` runs on the assets in `tests/data`. To also check a local folder of assets that must not
be committed, set `UASSET_TEST_DIR=/path/to/assets`.
