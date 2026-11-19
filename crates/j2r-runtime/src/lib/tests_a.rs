use super::*;

#[test]
fn cat2_flags() {
    let _rt = crate::runtime();
    assert!(Value::Long(0).cat2());
    assert!(Value::Double(0.0).cat2());
    assert!(!Value::Int(0).cat2());
    assert!(!Value::Ref(None).cat2());
}

#[test]
fn throw_new_sets_message_slot() {
    let _rt = crate::runtime();
    let t = throw_new("java/lang/RuntimeException", Some("boom"));
    assert_eq!(t.java_class(), "java/lang/RuntimeException");
    let msg = crate::heap::with_obj(t.obj, |o| o.slots[0].clone());
    match msg {
        Value::Ref(Some(s)) => assert_eq!(&*object::string_value(&s), "boom"),
        other => panic!("unexpected {other:?}"),
    }
}
/// R3 mitigation (M4_PLAN §2): every `thread_local!` remaining in
/// j2r-runtime must carry an `M4-SEAM` justification. Wave 1 migrated
/// the heap and all identity caches; frame.rs is the one documented
/// per-thread-by-design seam. A future unannotated seam fails CI here.
#[test]
fn thread_local_seams_are_annotated() {
    let _rt = crate::runtime();
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut seams = Vec::new();
    for entry in std::fs::read_dir(&src).expect("runtime src dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read src file");
        // Macro invocations only (line starts with `thread_local!`),
        // never prose mentions of the pattern.
        let invoked = text.lines().any(|l| l.starts_with("thread_local!"));
        if invoked {
            assert!(
                text.contains("M4-SEAM"),
                "unannotated thread_local! in {} — every remaining seam \
                 must carry an M4-SEAM justification (M4_PLAN §2 R3)",
                path.display()
            );
            seams.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    seams.sort();
    assert_eq!(
        seams,
        vec!["frame.rs".to_string(), "thread.rs".to_string()],
        "Wave 2 seams: frame.rs depth/limit scalars + thread.rs CURRENT \
         handle (both owner-only; all shared state lives in the registry)"
    );
}

/// T05: the shared containers are compile-proven Send+Sync — the
/// property Wave 2's real threads depend on. `Value`/`JObj` going
/// Send+Sync is what the Rc→Arc groundwork (W1a) bought.
#[test]
fn shared_containers_are_send_sync() {
    let _rt = crate::runtime();
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<crate::Value>();
    assert_send_sync::<crate::object::JObj>();
    assert_send_sync::<crate::heap::JRef>();
    // The Mutex<Heap> container assertion lives in heap.rs's own test
    // module, where the private Heap type is in scope.
}
