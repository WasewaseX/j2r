use super::testsupport::*;
use super::*;
#[test]
fn table_size_for_matches_openjdk() {
    let _rt = crate::runtime();
    assert_eq!(table_size_for(0), 1);
    assert_eq!(table_size_for(1), 1);
    assert_eq!(table_size_for(2), 2);
    assert_eq!(table_size_for(3), 4);
    assert_eq!(table_size_for(4), 4);
    assert_eq!(table_size_for(5), 8);
    assert_eq!(table_size_for(12), 16);
    assert_eq!(table_size_for(16), 16);
    assert_eq!(table_size_for(17), 32);
    assert_eq!(table_size_for(1000), 1024);
    assert_eq!(table_size_for(1 << 30), 1 << 30);
    assert_eq!(table_size_for((1 << 30) + 5), 1 << 30);
    assert_eq!(table_size_for(-7), 1);
}

// ---- hash spreading ----
#[test]
fn hash_spreading_matches_java() {
    let _rt = crate::runtime();
    // pinned by hand: 0x12345678 ^ (0x12345678 >>> 16) = 0x1234444C
    assert_eq!(
        map_hash(&boxed_i32(0x12345678u32 as i32), ctx()).unwrap(),
        0x1234444Cu32 as i32
    );
    // Integer hashCode == value -> small ints spread to themselves
    assert_eq!(map_hash(&boxed_i32(5), ctx()).unwrap(), 5);
    // negative hash: -5 = 0xFFFFFFFB; h>>>16 = 0xFFFF;
    // xor -> 0xFFFF0004 (hand-computed, then verified vs Temurin via
    // the differential suite)
    assert_eq!(map_hash(&boxed_i32(-5), ctx()).unwrap(), -65532);
}

// ---- ArrayList ----
#[test]
fn rooted_collection_keeps_elements_alive_across_gc() {
    let _rt = crate::runtime();
    crate::heap::set_forced_gc_every(Some(1));
    let l = list_new();
    let elem = new_string("kept-by-list");
    list_add(&l, Value::Ref(Some(elem)));
    // root ONLY the list (simulates a caller frame snapshot)
    let _nr = crate::frame::NativeRoots::push(&[l]);
    for _ in 0..50 {
        let _churn = new_string("churn");
        crate::heap::gc_point();
    }
    assert!(
        crate::heap::is_live(elem),
        "element rooted through the list survives"
    );
    assert!(crate::heap::is_live(l), "the list itself survives");
    // the element is intact and readable through the list
    let v = n_al_get(Some(&l), &[Value::Int(0)], ctx()).unwrap();
    assert_eq!(s(&v), "kept-by-list");
    crate::heap::set_forced_gc_every(None);
}
#[test]
fn dropped_collection_lets_elements_be_collected() {
    let _rt = crate::runtime();
    crate::heap::set_forced_gc_every(Some(1));
    let before = crate::heap::live_count();
    {
        let l = list_new();
        list_add(&l, Value::Ref(Some(new_string("dies-with-list"))));
        // HashMap with nodes
        let m = map_new();
        n_hm_put(
            Some(&m),
            &[Value::Ref(Some(new_string("k"))), boxed_i32(1)],
            ctx(),
        )
        .unwrap();
        // HashSet with its PRESENT dummy
        let st = set_new();
        n_hs_add(Some(&st), &[Value::Ref(Some(new_string("s")))], ctx()).unwrap();
    }
    crate::heap::gc_point();
    assert!(
        crate::heap::live_count() <= before,
        "dropped collections must be collectable whole (live {} before, {} after)",
        before,
        crate::heap::live_count()
    );
    crate::heap::set_forced_gc_every(None);
}

// ---- G3.3 slice 3: the node-stamped class tag + lazy materialization ----
