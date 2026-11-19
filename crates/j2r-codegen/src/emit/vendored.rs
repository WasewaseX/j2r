//! `emit`::vendored — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use std::path::Path;

pub(crate) fn copy_runtime(out: &Path) -> crate::Result<()> {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let crates_root = Path::new(manifest).join("..");
    // T09: the runtime gained an internal path dependency on the audited
    // FFI crate (j2r-jni-ffi, docs/M5_JNI_FFI.md) — a generated project
    // vendors BOTH crates, and the runtime manifest's `../j2r-jni-ffi`
    // path resolves inside the generated layout because the vendored
    // directory names mirror the workspace-relative names.
    for (crate_dir, vendored) in [("j2r-runtime", "runtime"), ("j2r-jni-ffi", "j2r-jni-ffi")] {
        let source = crates_root.join(crate_dir);
        let dest = out.join(vendored);
        // Idempotent vendoring: a file is copied only when its CONTENT differs
        // from what is already at the destination, and stale files that no
        // longer exist upstream are removed. An unchanged runtime therefore
        // keeps its mtimes, so cargo's fingerprints stay valid across repeated
        // emit_crate calls into the same out dir — the cfg-equiv harness
        // regenerates per fixture, and without this the vendored runtime (the
        // expensive compile unit) rebuilt 3 × fixtures times. A stale or
        // partially-written file fails the content compare and is re-copied:
        // self-healing. The walk is recursive since T08 (the runtime has
        // directory modules — `jni/`).
        std::fs::create_dir_all(&dest)?;
        copy_file_idempotent(&source.join("Cargo.toml"), &dest.join("Cargo.toml"))?;
        std::fs::create_dir_all(dest.join("src"))?;
        let mut upstream: Vec<std::path::PathBuf> = Vec::new();
        copy_dir_recursive(
            &source.join("src"),
            &dest.join("src"),
            Path::new(""),
            &mut upstream,
        )?;
        remove_stale_files(&dest.join("src"), &dest.join("src"), &upstream)?;
    }
    Ok(())
}

/// Recursively copy `src_dir` into `dst_dir`, mirroring subdirectories and
/// collecting every copied file's path relative to the src root.
pub(crate) fn copy_dir_recursive(
    src_dir: &Path,
    dst_dir: &Path,
    rel: &Path,
    upstream: &mut Vec<std::path::PathBuf>,
) -> crate::Result<()> {
    std::fs::create_dir_all(dst_dir)?;
    for entry in std::fs::read_dir(src_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let child_rel = rel.join(&name);
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_dir.join(&name), &child_rel, upstream)?;
        } else {
            upstream.push(child_rel);
            copy_file_idempotent(&entry.path(), &dst_dir.join(&name))?;
        }
    }
    Ok(())
}

/// Remove vendored files that no longer exist upstream (recursively), then
/// any directories left empty by those removals.
pub(crate) fn remove_stale_files(
    dir: &Path,
    src_root: &Path,
    upstream: &[std::path::PathBuf],
) -> crate::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            remove_stale_files(&path, src_root, upstream)?;
            if std::fs::read_dir(&path)?.next().is_none() {
                std::fs::remove_dir(&path)?;
            }
        } else {
            let rel = path.strip_prefix(src_root).unwrap_or(&path).to_path_buf();
            if !upstream.iter().any(|u| u == &rel) {
                std::fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

/// Copy `src` to `dst` only when the destination's content differs (missing,
/// stale, or partially written). Preserves mtimes — and with them cargo's
/// path-dep fingerprints — when nothing changed.
pub(crate) fn copy_file_idempotent(src: &Path, dst: &Path) -> crate::Result<()> {
    let want = std::fs::read(src)?;
    if let Ok(have) = std::fs::read(dst) {
        if have == want {
            return Ok(());
        }
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dst, want)?;
    Ok(())
}
