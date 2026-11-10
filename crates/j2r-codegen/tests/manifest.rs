//! Build-manifest tests (M4 T06/T07): per-class input-hash → emitted-hash
//! reuse, write-if-changed behavior, surface-gated invalidation, orphan
//! cleanup, and corrupt-manifest fallback.
//!
//! The fixtures are hand-assembled class files (the test counterpart of
//! javac — no JDK needed): `Util` (a static method), `App` (main calls
//! `Util.inc` and prints), and an independent `Extra` class for the
//! orphan-removal scenario.

mod common;

use common::builder::{method, Builder, ACC_PUBLIC, ACC_STATIC};
use j2r_codegen::{compile_to_crate_stats, Options};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "j2r-manifest-test-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A static `inc(I)I` class; `add` selects the method body (bodies with the
/// same signature but different bytecode drive the body-edit scenario).
fn static_method_class(name: &str, extra_method: bool, body: &[u8]) -> Vec<u8> {
    let mut b = Builder::new();
    let n_utf = b.utf8(name);
    let this = b.class(n_utf);
    let on = b.utf8("java/lang/Object");
    let super_cls = b.class(on);
    let inc_n = b.utf8("inc");
    let inc_d = b.utf8("(I)I");
    let code_n = b.utf8("Code");
    let inc = method(
        ACC_PUBLIC | ACC_STATIC,
        inc_n,
        inc_d,
        2,
        1,
        body,
        &[],
        code_n,
    );
    let methods: Vec<Vec<u8>> = if extra_method {
        let id_n = b.utf8("id");
        let id_d = b.utf8("(I)I");
        let id = method(
            ACC_PUBLIC | ACC_STATIC,
            id_n,
            id_d,
            1,
            1,
            &[0x1a, 0xac], // iload_0; ireturn
            &[],
            code_n,
        );
        vec![inc, id]
    } else {
        vec![inc]
    };
    let method_refs: Vec<&[u8]> = methods.iter().map(|m| m.as_slice()).collect();
    b.finish(ACC_PUBLIC, this, super_cls, &[], &[], &method_refs, &[])
}

/// `App` with main: getstatic System.out; bipush 41; invokestatic Util.inc;
/// invokevirtual println(I); return.
fn app_class() -> Vec<u8> {
    let mut b = Builder::new();
    let n_utf = b.utf8("App");
    let this = b.class(n_utf);
    let on = b.utf8("java/lang/Object");
    let super_cls = b.class(on);
    let util_utf = b.utf8("Util");
    let util_cls = b.class(util_utf);
    let inc_n = b.utf8("inc");
    let inc_d = b.utf8("(I)I");
    let inc_nat = b.name_and_type(inc_n, inc_d);
    let inc_ref = b.method_ref(util_cls, inc_nat);
    let ps_utf = b.utf8("java/io/PrintStream");
    let ps_cls = b.class(ps_utf);
    let out_n = b.utf8("out");
    let ps_desc = b.utf8("Ljava/io/PrintStream;");
    let ps_nat = b.name_and_type(out_n, ps_desc);
    let sys_utf = b.utf8("java/lang/System");
    let sys_cls = b.class(sys_utf);
    let sys_nat = b.name_and_type(out_n, ps_desc);
    let sys_out = b.field_ref(sys_cls, sys_nat);
    let pln_n = b.utf8("println");
    let pln_d = b.utf8("(I)V");
    let pln_nat = b.name_and_type(pln_n, pln_d);
    let pln_ref = b.method_ref(ps_cls, pln_nat);
    let main_n = b.utf8("main");
    let main_d = b.utf8("([Ljava/lang/String;)V");
    let code_n = b.utf8("Code");
    let main = method(
        ACC_PUBLIC | ACC_STATIC,
        main_n,
        main_d,
        3,
        1,
        &[
            0xB2,
            ((sys_out >> 8) as u8),
            (sys_out as u8), // getstatic System.out
            0x10,
            41, // bipush 41
            0xB8,
            ((inc_ref >> 8) as u8),
            (inc_ref as u8), // invokestatic Util.inc(I)I
            0xB6,
            ((pln_ref >> 8) as u8),
            (pln_ref as u8), // invokevirtual println(I)
            0xB1,            // return
        ],
        &[],
        code_n,
    );
    let _ = ps_nat;
    b.finish(ACC_PUBLIC, this, super_cls, &[], &[], &[&main], &[])
}

fn write_class(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(format!("{name}.class"));
    std::fs::write(&p, bytes).unwrap();
    p
}

fn compile(inputs: Vec<PathBuf>, out: &Path) -> j2r_codegen::manifest::EmitStats {
    let opts = Options {
        inputs,
        classpath: Vec::new(),
        main_class: None,
        crate_name: "j2r-manifest-test".to_string(),
        out_dir: out.to_path_buf(),
        emit_cfg: false,
        emit_typed: false,
        partition: None,
    };
    compile_to_crate_stats(&opts).expect("codegen succeeds").1
}

/// G2.2 (F2): the (mtime,size) fast path trusts a stat match even when the
/// recorded content hash is wrong (proving the hash was skipped), and a
/// stat change falls back to the content hash, which forces a re-emit —
/// never a silent stale reuse.
#[test]
fn stat_fast_path_trusts_then_falls_back_to_hash() {
    let dir = temp_dir("statfp");
    let out = dir.join("out");
    let util = write_class(
        &dir,
        "Util",
        &static_method_class("Util", false, &[0x1a, 0x04, 0x60, 0xac]),
    );
    let app = write_class(&dir, "App", &app_class());
    let inputs = || vec![util.clone(), app.clone()];
    compile(inputs(), &out);

    // tamper the manifest: keep App's stat field, corrupt its emitted sha
    let mpath = j2r_codegen::manifest::Manifest::path(&out);
    let text = std::fs::read_to_string(&mpath).unwrap();
    let mut tampered = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("e\tApp\t") {
            let f: Vec<&str> = rest.split('\t').collect();
            assert!(f.len() >= 3, "v2 e line carries the stat");
            tampered.push_str(&format!(
                "e\tApp\t{}\t{}\t{}\n",
                f[0],
                "ff".repeat(32),
                f[2]
            ));
        } else {
            tampered.push_str(line);
            tampered.push('\n');
        }
    }
    std::fs::write(&mpath, tampered).unwrap();

    // stat matches (file untouched) → the fast path trusts it and App is
    // reused DESPITE the wrong content hash
    let s2 = compile(inputs(), &out);
    assert_eq!(s2.classes_reused, 2, "stat match must take the fast path");

    // change App.rs's mtime only (content stays identical): stat misses →
    // content-hash fallback → the TAMPERED sha mismatches the (identical)
    // content → one re-render, which is a cost, never a correctness issue
    // (rendering is deterministic); the repaired entry records the correct
    // sha + the new stat.
    let app_rs = out.join("src/classes/App.rs");
    let f = std::fs::File::options().write(true).open(&app_rs).unwrap();
    f.set_times(
        std::fs::FileTimes::new()
            .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_730_000_000)),
    )
    .unwrap();
    drop(f);
    let s3 = compile(inputs(), &out);
    assert_eq!(
        s3.classes_reused, 1,
        "Util (stat+hash fine) reuses; App's tampered sha + new mtime re-renders"
    );
    assert_eq!(s3.classes_reemitted, 1);

    // the manifest is repaired now (correct sha, current stat) — a REAL
    // content change must still be caught: rewrite App.rs to garbage
    // (stat changes → hash decides → mismatch → re-emit)
    std::fs::write(&app_rs, b"garbage").unwrap();
    let s4 = compile(inputs(), &out);
    assert_eq!(s4.classes_reused, 1, "content change must force re-emit");
    assert_eq!(s4.classes_reemitted, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

fn compile_partitioned(
    inputs: Vec<PathBuf>,
    out: &Path,
    k: usize,
) -> j2r_codegen::manifest::EmitStats {
    let opts = Options {
        inputs,
        classpath: Vec::new(),
        main_class: None,
        crate_name: "j2r-manifest-test".to_string(),
        out_dir: out.to_path_buf(),
        emit_cfg: false,
        emit_typed: false,
        partition: Some(k),
    };
    compile_to_crate_stats(&opts).expect("codegen succeeds").1
}

fn mtime(p: &Path) -> SystemTime {
    std::fs::metadata(p).unwrap().modified().unwrap()
}

#[test]
fn reuse_on_identical_inputs() {
    let dir = temp_dir("reuse");
    let out = dir.join("out");
    let util = write_class(
        &dir,
        "Util",
        &static_method_class("Util", false, &[0x1a, 0x04, 0x60, 0xac]),
    );
    let app = write_class(&dir, "App", &app_class());

    let s1 = compile(vec![util.clone(), app.clone()], &out);
    assert_eq!(s1.classes_reused, 0, "cold compile reuses nothing");
    assert_eq!(s1.classes_reemitted, 2);
    assert!(!s1.had_manifest);

    let app_rs = out.join("src/classes/App.rs");
    let util_rs = out.join("src/classes/Util.rs");
    let mod_rs = out.join("src/classes/mod.rs");
    let t_app = mtime(&app_rs);
    let t_util = mtime(&util_rs);
    let t_mod = mtime(&mod_rs);

    let s2 = compile(vec![util, app], &out);
    assert_eq!(s2.classes_reused, 2, "identical inputs must reuse both");
    assert_eq!(s2.classes_reemitted, 0);
    assert!(s2.had_manifest);
    assert_eq!(s2.files_written, 0, "no file may be rewritten on a no-op");
    assert_eq!(mtime(&app_rs), t_app, "reused file must not be touched");
    assert_eq!(mtime(&util_rs), t_util, "reused file must not be touched");
    assert_eq!(mtime(&mod_rs), t_mod, "identical glue must not be touched");

    // manifest parses and records both classes
    let m = j2r_codegen::manifest::Manifest::load(&out).expect("manifest loads");
    assert_eq!(m.entries.len(), 2);
    assert!(m.entries.contains_key("App") && m.entries.contains_key("Util"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn manifest_roundtrip() {
    let mut m = j2r_codegen::manifest::Manifest {
        emitter: 7,
        emit_cfg: true,
        emit_typed: true,
        surface: "ab".repeat(32),
        layout: "cd".repeat(32),
        crate_name: "demo".to_string(),
        entries: Default::default(),
        glue: Default::default(),
    };
    m.entries.insert(
        "p/Big".to_string(),
        j2r_codegen::manifest::Entry {
            input_sha: "11".repeat(32),
            emitted_sha: "22".repeat(32),
            emitted_stat: Some((1_731_234_567_890_123_456, 4096)),
        },
    );
    m.glue.insert("src/lib.rs".to_string(), "33".repeat(32));
    let text = m.render();
    let parsed = j2r_codegen::manifest::parse(text.as_bytes()).expect("parses");
    assert_eq!(parsed, m);
    // a legacy 4-field e line parses with the stat absent (v1 manifests)
    let legacy = format!(
        "v2\temitter=2\temit_cfg=false\temit_typed=false\tsurface={}\tlayout=-\tcrate=demo\ne\tp/Big\t{}\t{}\n",
        "ab".repeat(32),
        "11".repeat(32),
        "22".repeat(32)
    );
    let parsed_legacy = j2r_codegen::manifest::parse(legacy.as_bytes()).expect("legacy parses");
    assert_eq!(parsed_legacy.entries["p/Big"].emitted_stat, None);
    // a malformed 5th field is corruption, not a missing stat
    let corrupt = legacy.replace(
        &format!("e\tp/Big\t{}\t{}\n", "11".repeat(32), "22".repeat(32)),
        &format!("e\tp/Big\t{}\t{}\tzz.\n", "11".repeat(32), "22".repeat(32)),
    );
    assert!(j2r_codegen::manifest::parse(corrupt.as_bytes()).is_none());
    // garbage must parse to None, never panic
    assert!(j2r_codegen::manifest::parse(b"hello world".as_slice()).is_none());
    assert!(j2r_codegen::manifest::parse(b"v1\temitter=x\n".as_slice()).is_none());
    assert!(j2r_codegen::manifest::parse(
        "v1\temitter=1\temit_cfg=false\tsurface=zz\ncrate=c\n".as_bytes()
    )
    .is_none());
}

#[test]
fn body_edit_reemits_only_edited_class() {
    let dir = temp_dir("body");
    let out = dir.join("out");
    let app = write_class(&dir, "App", &app_class());
    let util_v1 = write_class(
        &dir,
        "Util",
        &static_method_class("Util", false, &[0x1a, 0x04, 0x60, 0xac]),
    );

    compile(vec![util_v1.clone(), app.clone()], &out);
    let app_rs = out.join("src/classes/App.rs");
    let util_rs = out.join("src/classes/Util.rs");
    let app_v1 = std::fs::read_to_string(&app_rs).unwrap();
    let util_v1_rs = std::fs::read_to_string(&util_rs).unwrap();
    let t_app = mtime(&app_rs);

    // edit Util's BODY only (same signature): iload_0; bipush 7; iadd; ireturn
    let util_v2 = write_class(
        &dir,
        "Util",
        &static_method_class("Util", false, &[0x1a, 0x10, 0x07, 0x60, 0xac]),
    );
    let s2 = compile(vec![util_v2, app], &out);

    assert_eq!(s2.classes_reused, 1, "App must be reused");
    assert_eq!(s2.classes_reemitted, 1, "Util must be re-emitted");
    assert_eq!(s2.orphans_removed, 0);
    let app_v2 = std::fs::read_to_string(&app_rs).unwrap();
    assert_eq!(app_v1, app_v2, "App's emitted code must be byte-identical");
    assert_eq!(mtime(&app_rs), t_app, "App.rs must not be rewritten");
    let util_v2_rs = std::fs::read_to_string(&util_rs).unwrap();
    assert_ne!(util_v1_rs, util_v2_rs, "Util's emitted code must change");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn surface_change_reemits_all() {
    let dir = temp_dir("surface");
    let out = dir.join("out");
    let app = write_class(&dir, "App", &app_class());
    let util_v1 = write_class(
        &dir,
        "Util",
        &static_method_class("Util", false, &[0x1a, 0x04, 0x60, 0xac]),
    );
    compile(vec![util_v1.clone(), app.clone()], &out);

    // signature change: Util gains a static method `id`
    let util_v3 = write_class(
        &dir,
        "Util",
        &static_method_class("Util", true, &[0x1a, 0x04, 0x60, 0xac]),
    );
    let s2 = compile(vec![util_v3, app], &out);
    assert_eq!(s2.classes_reused, 0, "surface change must invalidate all");
    assert_eq!(s2.classes_reemitted, 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn removed_class_orphan_deleted() {
    let dir = temp_dir("orphan");
    let out = dir.join("out");
    let util = write_class(
        &dir,
        "Util",
        &static_method_class("Util", false, &[0x1a, 0x04, 0x60, 0xac]),
    );
    let app = write_class(&dir, "App", &app_class());
    let extra = write_class(
        &dir,
        "Extra",
        &static_method_class("Extra", false, &[0x1a, 0xac]),
    );

    compile(vec![util.clone(), app.clone(), extra], &out);
    assert!(out.join("src/classes/Extra.rs").exists());

    let s2 = compile(vec![util, app], &out);
    assert!(
        !out.join("src/classes/Extra.rs").exists(),
        "orphaned class file must be removed"
    );
    assert!(s2.orphans_removed >= 1);
    let registry = std::fs::read_to_string(out.join("src/registry.rs")).unwrap();
    assert!(
        !registry.contains("Extra"),
        "registry must drop the removed class"
    );
    let m = j2r_codegen::manifest::Manifest::load(&out).unwrap();
    assert!(
        !m.entries.contains_key("Extra"),
        "manifest must prune the removed class"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn corrupt_manifest_falls_back_to_full_reemit() {
    let dir = temp_dir("corrupt");
    let out = dir.join("out");
    let util = write_class(
        &dir,
        "Util",
        &static_method_class("Util", false, &[0x1a, 0x04, 0x60, 0xac]),
    );
    let app = write_class(&dir, "App", &app_class());
    compile(vec![util.clone(), app.clone()], &out);

    std::fs::write(
        out.join("j2r-manifest.tsv"),
        b"not a manifest at all\n\x00\tgarbage",
    )
    .unwrap();
    let s2 = compile(vec![util, app], &out);
    assert_eq!(s2.classes_reused, 0, "corrupt manifest must disable reuse");
    assert_eq!(s2.classes_reemitted, 2);
    // and the manifest is rewritten valid for the next run
    let s3_manifest = j2r_codegen::manifest::Manifest::load(&out);
    assert!(s3_manifest.is_some(), "manifest must be valid again");
    let m = s3_manifest.unwrap();
    assert_eq!(m.entries.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn partitioned_reuse_and_body_edit_isolation() {
    let dir = temp_dir("part");
    let out = dir.join("out");
    let app = write_class(&dir, "App", &app_class());
    let util_v1 = write_class(
        &dir,
        "Util",
        &static_method_class("Util", false, &[0x1a, 0x04, 0x60, 0xac]),
    );

    // k=1: two classes -> two part crates; App calls Util cross-partition
    // (through the link table), so this exercises the seam path too
    let s1 = compile_partitioned(vec![util_v1.clone(), app.clone()], &out, 1);
    assert_eq!(s1.classes_reemitted, 2);
    assert!(out.join("p0").exists() && out.join("p1").exists());
    let manifest = j2r_codegen::manifest::Manifest::load(&out).unwrap();
    assert_ne!(manifest.layout, "-");

    let p0_lib = out.join("p0/src/lib.rs");
    let t_p0_lib = mtime(&p0_lib);

    let s2 = compile_partitioned(vec![util_v1.clone(), app.clone()], &out, 1);
    assert_eq!(s2.classes_reused, 2, "identical inputs reuse across parts");
    assert_eq!(s2.files_written, 0);
    assert_eq!(
        mtime(&p0_lib),
        t_p0_lib,
        "untouched part lib.rs keeps mtime"
    );

    // body edit of Util: only Util re-emits; App and the p0 glue stay put
    // (the part lib.rs files depend on signatures only, which did not move)
    let util_v2 = write_class(
        &dir,
        "Util",
        &static_method_class("Util", false, &[0x1a, 0x10, 0x07, 0x60, 0xac]),
    );
    let s3 = compile_partitioned(vec![util_v2, app], &out, 1);
    assert_eq!(s3.classes_reused, 1, "App must be reused");
    assert_eq!(s3.classes_reemitted, 1, "Util must re-emit");
    assert_eq!(
        mtime(&p0_lib),
        t_p0_lib,
        "p0 glue untouched by a p1 body edit"
    );

    // signature change: global gate refuses reuse — conservative re-emit
    let _util_v3 = write_class(
        &dir,
        "Util",
        &static_method_class("Util", true, &[0x1a, 0x04, 0x60, 0xac]),
    );
    let s4 = compile_partitioned(vec![dir.join("Util.class"), dir.join("App.class")], &out, 1);
    assert_eq!(s4.classes_reused, 0);
    assert_eq!(s4.classes_reemitted, 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn partition_budget_change_invalidates_gate() {
    let dir = temp_dir("partk");
    let out = dir.join("out");
    let _app = write_class(&dir, "App", &app_class());
    let _util = write_class(
        &dir,
        "Util",
        &static_method_class("Util", false, &[0x1a, 0x04, 0x60, 0xac]),
    );
    let _extra = write_class(
        &dir,
        "Extra",
        &static_method_class("Extra", false, &[0x1a, 0xac]),
    );
    let inputs = || {
        vec![
            dir.join("Util.class"),
            dir.join("App.class"),
            dir.join("Extra.class"),
        ]
    };
    compile_partitioned(inputs(), &out, 1); // 3 parts
                                            // same inputs, different budget: layout reshuffles -> full re-emit
    let s2 = compile_partitioned(inputs(), &out, 2); // 2 parts
    assert_eq!(
        s2.classes_reused, 0,
        "budget change must invalidate the gate"
    );
    assert_eq!(s2.classes_reemitted, 3);
    let s3 = compile_partitioned(inputs(), &out, 2);
    assert_eq!(s3.classes_reused, 3, "stable budget reuses again");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn emit_cfg_change_invalidates_gate() {
    let dir = temp_dir("emitcfg");
    let out = dir.join("out");
    let util = write_class(
        &dir,
        "Util",
        &static_method_class("Util", false, &[0x1a, 0x04, 0x60, 0xac]),
    );
    let app = write_class(&dir, "App", &app_class());
    compile(vec![util.clone(), app.clone()], &out);

    // same inputs, different option: the gate must refuse reuse (the
    // manifest records emit_cfg; the surface hash covers it too)
    let opts = Options {
        inputs: vec![util.clone(), app.clone()],
        classpath: Vec::new(),
        main_class: None,
        crate_name: "j2r-manifest-test".to_string(),
        out_dir: out.clone(),
        emit_cfg: true,
        emit_typed: false,
        partition: None,
    };
    let (_dir, s2) = compile_to_crate_stats(&opts).unwrap();
    assert_eq!(s2.classes_reused, 0);
    assert_eq!(s2.classes_reemitted, 2);
    // switching back: the gate compares against the LAST run (recorded with
    // emit_cfg=true), so this pass re-renders the dispatch bodies...
    let s3 = compile(vec![util.clone(), app.clone()], &out);
    assert_eq!(s3.classes_reused, 0, "gate is against the last run's shape");
    assert_eq!(s3.classes_reemitted, 2);
    // ...and from the next run on, reuse returns (files now match the
    // recorded emit_cfg=false surface again)
    let s4 = compile(vec![util, app], &out);
    assert_eq!(
        s4.classes_reused, 2,
        "back to the recorded shape: reuse returns"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hand_edited_emitted_file_is_not_reused() {
    let dir = temp_dir("handedit");
    let out = dir.join("out");
    let util = write_class(
        &dir,
        "Util",
        &static_method_class("Util", false, &[0x1a, 0x04, 0x60, 0xac]),
    );
    let app = write_class(&dir, "App", &app_class());
    compile(vec![util.clone(), app.clone()], &out);

    // someone edits a generated file behind j2r's back (append bytes so the
    // disk hash provably diverges from the recorded emitted hash)
    let util_rs = out.join("src/classes/Util.rs");
    let mut edited = std::fs::read_to_string(&util_rs).unwrap();
    edited.push_str("\n// hand edit: ICONST_STOLEN\n");
    std::fs::write(&util_rs, edited).unwrap();

    let s2 = compile(vec![util, app], &out);
    assert_eq!(s2.classes_reused, 1, "App still verifies");
    assert_eq!(
        s2.classes_reemitted, 1,
        "Util's disk hash mismatch forces re-emit"
    );
    let back = std::fs::read_to_string(&util_rs).unwrap();
    assert!(
        !back.contains("ICONST_STOLEN"),
        "the emitter must overwrite the hand edit with correct output"
    );
    // strongest form: the on-disk file is again exactly what the manifest
    // records (the same bytes a clean emit would produce)
    let m = j2r_codegen::manifest::Manifest::load(&out).unwrap();
    let on_disk = j2r_codegen::sha256::sha256_hex(back.as_bytes());
    assert_eq!(on_disk, m.entries["Util"].emitted_sha);
    let _ = std::fs::remove_dir_all(&dir);
}
