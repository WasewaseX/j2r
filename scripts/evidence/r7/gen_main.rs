//! R7 measurement generator: emits the OLD (string-tuple match) and NEW
//! (hash-led match, via the REAL dispatch_table::emit_lookup_fn) lookup
//! sources for the same N synthetic entries, plus the A/B bench crate.
//! Measurement scaffolding only — never shipped, never vendored.

use j2r_codegen::dispatch_table::{emit_lookup_fn, LookupEntry};
use std::fmt::Write;

const N: usize = 2169;

fn class_name(i: usize) -> String {
    format!("org/jbox2d/dynamics/Body{}", i % 97)
}

fn method_name(i: usize) -> String {
    format!("m{i}")
}

fn desc(i: usize) -> String {
    format!("(FII){}", ["V", "I", "J", "D"][i % 4])
}

fn entries() -> Vec<LookupEntry> {
    (0..N)
        .map(|i| LookupEntry {
            class: class_name(i),
            name: method_name(i),
            desc: desc(i),
            target: format!("crate::payload{i}()"),
        })
        .collect()
}

fn dummy_fns(out: &mut String) {
    for i in 0..N {
        let _ = writeln!(out, "pub fn payload{i}() -> ::core::primitive::u32 {{ {i}u32 }}");
    }
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    std::fs::create_dir_all(&dir).unwrap();

    // ---- OLD shape: the pre-R7 emission, reproduced verbatim ----------
    let mut old = String::new();
    let _ = writeln!(old, "// pre-R7 shape, reproduced for measurement");
    let _ = writeln!(
        old,
        "pub fn lookup(class: &str, name: &str, desc: &str) -> ::core::option::Option<::core::primitive::u32> {{"
    );
    let _ = writeln!(old, "    match (class, name, desc) {{");
    for i in 0..N {
        let _ = writeln!(
            old,
            "        ({:?}, {:?}, {:?}) => ::core::option::Option::Some(crate::payload{}()),",
            class_name(i),
            method_name(i),
            desc(i),
            i
        );
    }
    let _ = writeln!(old, "        _ => ::core::option::Option::None,");
    let _ = writeln!(old, "    }}");
    let _ = writeln!(old, "}}");
    std::fs::write(format!("{dir}/old_lookup_part.rs"), &old).unwrap();
    old.clear();
    dummy_fns(&mut old);
    std::fs::write(format!("{dir}/dummies.rs"), old.as_bytes()).unwrap();

    // compile unit: lookup + dummies
    let mut old_full = std::fs::read_to_string(format!("{dir}/old_lookup_part.rs")).unwrap();
    old_full.push_str(&std::fs::read_to_string(format!("{dir}/dummies.rs")).unwrap());
    std::fs::write(format!("{dir}/old_shape.rs"), old_full.clone()).unwrap();

    // ---- NEW shape: the REAL emitter, targets de-typed for the bench --
    let mut buf = String::new();
    emit_lookup_fn(&mut buf, &entries());
    let buf = buf
        .replace(" as ::j2r_runtime::MethodFn", "")
        .replace("::j2r_runtime::MethodFn", "::core::primitive::u32");
    let new_full = format!("// R7 hash-led shape, real emitter\n{buf}{}", {
        let mut d = String::new();
        dummy_fns(&mut d);
        d
    });
    std::fs::write(format!("{dir}/new_shape.rs"), new_full.clone()).unwrap();

    // ---- A/B bench: both lookups, renamed, shared payloads ------------
    let lookup_only = |src: &str, cut_at: &str| {
        let cut = src.find(cut_at).unwrap();
        src[..cut].to_string()
    };
    let old_lookup = lookup_only(&old_full, "pub fn payload0").replace("pub fn lookup(", "pub fn lookup_old(");
    let new_lookup = lookup_only(&new_full, "pub fn payload0").replace("pub fn lookup(", "pub fn lookup_new(");
    let mut dummies = String::new();
    dummy_fns(&mut dummies);
    let bench_body = format!(
        r#"
pub fn bench_main() {{
    let hits: Vec<(String, String, String)> = (0..{N})
        .map(|i| (class_of(i), name_of(i), desc_of(i)))
        .collect();
    let misses: Vec<(String, String, String)> = (0..{N})
        .map(|i| (format!("org/absent/{{}}", i), name_of(i), desc_of(i)))
        .collect();
    let rounds = 2000u32;
    let mut sink = 0u32;
    for k in 0..{N} {{
        if let Some(v) = lookup_old(&hits[k].0, &hits[k].1, &hits[k].2) {{ sink ^= v; }}
    }}
    let t0 = std::time::Instant::now();
    for _ in 0..rounds {{
        for k in 0..{N} {{
            if let Some(v) = lookup_old(&hits[k].0, &hits[k].1, &hits[k].2) {{ sink ^= v; }}
        }}
        for k in 0..{N} {{
            if lookup_old(&misses[k].0, &misses[k].1, &misses[k].2).is_some() {{ sink ^= 1; }}
        }}
    }}
    let old_t = t0.elapsed();
    let t1 = std::time::Instant::now();
    for _ in 0..rounds {{
        for k in 0..{N} {{
            if let Some(v) = lookup_new(&hits[k].0, &hits[k].1, &hits[k].2) {{ sink ^= v; }}
        }}
        for k in 0..{N} {{
            if lookup_new(&misses[k].0, &misses[k].1, &misses[k].2).is_some() {{ sink ^= 1; }}
        }}
    }}
    let new_t = t1.elapsed();
    println!(
        "old {{:.1}} ms | new {{:.1}} ms | sink {{}}",
        old_t.as_secs_f64() * 1000.0,
        new_t.as_secs_f64() * 1000.0,
        sink
    );
}}

pub fn class_of(i: usize) -> String {{ format!("org/jbox2d/dynamics/Body{{}}", i % 97) }}
pub fn name_of(i: usize) -> String {{ format!("m{{}}", i) }}
pub fn desc_of(i: usize) -> String {{ format!("(FII){{}}", ["V", "I", "J", "D"][i % 4]) }}
pub fn main() {{ bench_main() }}
"#
    );
    std::fs::write(
        format!("{dir}/bench.rs"),
        format!("{old_lookup}\n{new_lookup}\n{dummies}\n{bench_body}"),
    )
    .unwrap();
    println!("generated old_shape.rs new_shape.rs bench.rs (N={N}) in {dir}");
}
