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
| `--nodes` | print graphs as numbered node listings instead of pseudocode |
| `--no-other` | hide objects that are not part of the main view |
| `--max-string <N>` | shorten long strings (default 2000, 0 = unlimited) |

## What is shown

- **Blueprints** (actor, widget, anim, editor utility, ...): parent class, interfaces, variables with
  types, defaults and flags, component tree (SCS) and inherited component overrides, widget tree with
  slot settings, class defaults, and every graph as pseudocode:

  ```
  event ReceiveBeginPlay() [override] {
      super.ReceiveBeginPlay()
      goto L1_MacroForEachLoop
  }

  event OnPlayerInitialized() [override] {
      super.OnPlayerInitialized()
      if (HasAuthority()) {
          goto L1_MacroForEachLoop
      }
  }

  L1_MacroForEachLoop: {
      for (Element, Index in CapsuleComponent.GetOverlappingActors(ClassFilter: /Game/.../BP_Volume.BP_Volume_C)) {
          if (let AsBPVolume = cast<BP_Volume_C>(Element)) {
              AsBPVolume.HandleOverlapBegin(Character: self)
          }
      }
  }
  ```
  Exec wires become statements (`if`, `switch`, loops for the standard macros, `await` for latent
  calls, `goto` to labelled shared blocks where wires merge), pure nodes become inline expressions,
  math library calls become operators. Overrides that only call `super` are omitted (listed in a
  comment). Anim graphs print the pose pipeline as `let` statements; state machines list states,
  aliases and transitions with their rule expressions. `--nodes` prints the raw node listing instead.
- **Materials / material functions**: settings, comments, parameters and HLSL-like code for every
  connected output (`BaseColor = lerp(...)`); shared expressions become `let`s and expressions not
  connected to any output are listed.
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
