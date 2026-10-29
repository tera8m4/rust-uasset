//! Development tool: decodes many assets and reports decoder failures.
use std::collections::BTreeMap;

use rust_uasset::{object, package::Package, property::Value};

fn count_raw(v: &Value, out: &mut BTreeMap<String, usize>, examples: &mut BTreeMap<String, String>, ctx: &str) {
    match v {
        Value::Raw { type_name, note, .. } => {
            let key = format!("{type_name} :: {}", note.split(" at ").next().unwrap_or(note));
            *out.entry(key.clone()).or_default() += 1;
            examples.entry(key).or_insert_with(|| ctx.to_string());
        }
        Value::Struct(s) => s.fields.iter().for_each(|f| count_raw(&f.value, out, examples, ctx)),
        Value::Array(a) | Value::Set(a) => a.iter().for_each(|x| count_raw(x, out, examples, ctx)),
        Value::Map(m) => m.iter().for_each(|(k, x)| {
            count_raw(k, out, examples, ctx);
            count_raw(x, out, examples, ctx)
        }),
        Value::Optional(Some(b)) => count_raw(b, out, examples, ctx),
        _ => {}
    }
}

fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if matches!(p.extension().and_then(|s| s.to_str()), Some("uasset") | Some("umap")) {
                out.push(p);
            }
        }
    }
}

fn main() {
    let mut files = vec![];
    for a in std::env::args().skip(1) {
        let p = std::path::PathBuf::from(&a);
        if p.is_dir() { walk(&p, &mut files) } else { files.push(p) }
    }
    let mut header_err: BTreeMap<String, usize> = BTreeMap::new();
    let mut obj_err: BTreeMap<String, usize> = BTreeMap::new();
    let mut raw: BTreeMap<String, usize> = BTreeMap::new();
    let mut und: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: BTreeMap<String, String> = BTreeMap::new();
    let (mut ok, mut total_exports) = (0, 0);
    for f in &files {
        let data = match std::fs::read(f) { Ok(d) => d, Err(_) => continue };
        let pkg = match Package::from_bytes(data) {
            Ok(p) => p,
            Err(e) => {
                let k = e.to_string();
                examples.entry(k.clone()).or_insert(f.display().to_string());
                *header_err.entry(k).or_default() += 1;
                continue;
            }
        };
        if pkg.uses_unversioned_properties() { continue; }
        ok += 1;
        for w in &pkg.warnings {
            let k = format!("pkg warning: {}", w.split(" at ").next().unwrap());
            examples.entry(k.clone()).or_insert(f.display().to_string());
            *header_err.entry(k).or_default() += 1;
        }
        for o in object::read_all(&pkg) {
            total_exports += 1;
            let ctx = format!("{} [{}] {}", f.display(), o.index, pkg.export_rel_path(o.index));
            for e in &o.errors {
                let k = format!("{} :: {}", o.class, e.split(" at ").next().unwrap().chars().take(90).collect::<String>());
                examples.entry(k.clone()).or_insert(ctx.clone());
                *obj_err.entry(k).or_default() += 1;
            }
            if o.undecoded > 0 && o.errors.is_empty() {
                let k = o.class.clone();
                examples.entry(format!("und {k}")).or_insert(format!("{ctx} ({} bytes: {})", o.undecoded, o.undecoded_preview));
                *und.entry(k).or_default() += 1;
            }
            for p in &o.properties { count_raw(&p.value, &mut raw, &mut examples, &ctx); }
            for x in &o.extras {
                if let object::Extra::DataTable(rows) = x {
                    for (_, props) in rows { for p in props { count_raw(&p.value, &mut raw, &mut examples, &ctx); } }
                }
            }
        }
    }
    println!("files: {} parsed: {} exports: {}", files.len(), ok, total_exports);
    let show = |title: &str, m: &BTreeMap<String, usize>, prefix: &str| {
        println!("== {title}");
        let mut v: Vec<_> = m.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        for (k, n) in v.iter().take(60) {
            println!("{n:6}  {k}\n          e.g. {}", examples.get(&format!("{prefix}{k}")).map(|s| s.as_str()).unwrap_or(""));
        }
    };
    show("header errors", &header_err, "");
    show("object errors", &obj_err, "");
    show("raw property values", &raw, "");
    show("undecoded tails (by class)", &und, "und ");
}
