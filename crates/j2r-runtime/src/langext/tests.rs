use super::*;

fn string_recv(text: &str) -> JRef {
    crate::heap::alloc(crate::object::JObj {
        jclass: Arc::from("java/lang/String"),
        slots: vec![Value::Str(Arc::new(text.to_string()))],
        prim: None,
    })
}

/// The [C representation law (the run-2 P0): toCharArray produces the
/// CANONICAL prim-payload object shape — slots = [length] only, the
/// UTF-16 units in a PrimData::I payload — the exact object shape
/// array_new_prim("[C", len) builds. The former prim: None boxed-slot
/// form crashed the typed backend's raw accessors (the payload
/// contract) on the first caload over a toCharArray result.
#[test]
fn to_char_array_is_payload_shaped() {
    let _rt = crate::runtime();
    let recv = string_recv("ab");
    let v = n_str_to_char_array(Some(&recv), &[], Dispatch::default()).unwrap();
    let r = v.expect_ref().unwrap();
    let (jclass, slots, prim_ok) = with_obj(r, |o| {
        (
            o.jclass.to_string(),
            o.slots.clone(),
            matches!(o.prim, Some(crate::object::PrimData::I(_))),
        )
    });
    assert_eq!(jclass, "[C");
    assert_eq!(slots.len(), 1, "payload arrays keep exactly one slot");
    assert_eq!(slots[0], Value::Int(2), "slot 0 is the length");
    assert!(prim_ok, "the payload IS the PrimData::I form");
    let units = with_obj(r, |o| match &o.prim {
        Some(crate::object::PrimData::I(v)) => v.clone(),
        other => panic!("unexpected payload {other:?}"),
    });
    assert_eq!(units, vec![0x61, 0x62], "UTF-16 units, char-narrowed");
}

/// Both accessor families read the SAME numbers off a toCharArray
/// array: the Value path (array_load) and the typed backend's raw
/// path (array_load_i) — including the surrogate-pair case (one code
/// point = two UTF-16 units).
#[test]
fn to_char_array_reads_agree_on_both_families() {
    let _rt = crate::runtime();
    let recv = string_recv("\u{1F600}x");
    let v = n_str_to_char_array(Some(&recv), &[], Dispatch::default()).unwrap();
    let r = v.expect_ref().unwrap();
    assert_eq!(crate::array::array_length(&r).unwrap(), 3);
    for (i, want) in [0xD83D, 0xDE00, 0x78].iter().enumerate() {
        let via_value = crate::array::array_load(&r, i as i32).unwrap();
        assert_eq!(via_value, Value::Int(*want), "Value path at {i}");
        let via_raw = crate::array::array_load_i(&r, i as i32).unwrap();
        assert_eq!(via_raw, *want, "raw path at {i}");
    }
}

/// The store-side law on a toCharArray array: the raw int-group store
/// narrows by the array's own descriptor (the castore law,
/// array_store_i_raw's CHAR arm) and the Value path reads it back.
#[test]
fn to_char_array_raw_store_narrows_by_descriptor() {
    let _rt = crate::runtime();
    let recv = string_recv("ab");
    let v = n_str_to_char_array(Some(&recv), &[], Dispatch::default()).unwrap();
    let r = v.expect_ref().unwrap();
    crate::array::array_store_i_raw(&r, 0, 66).unwrap(); // 'B'
    crate::array::array_store_i_raw(&r, 1, 65537).unwrap(); // (v as u16) as i32 == 1
    assert_eq!(crate::array::array_load_i(&r, 0).unwrap(), 66);
    assert_eq!(crate::array::array_load(&r, 0).unwrap(), Value::Int(66));
    assert_eq!(crate::array::array_load_i(&r, 1).unwrap(), 1);
    assert_eq!(crate::array::array_load(&r, 1).unwrap(), Value::Int(1));
}
