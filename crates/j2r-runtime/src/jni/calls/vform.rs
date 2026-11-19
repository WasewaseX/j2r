use super::*;

/// The A-form invoke core (`Call*MethodA`), shared by all three families.
/// NEVER fails: every Java-observable failure pends a throwable and
/// answers the zero value — the JNI-faithful shape the wrappers rely on.
pub(crate) fn invoke_a(
    family: CallFamily,
    mid: u64,
    receiver_tag: u64,
    declared_tag: u64,
    args_ptr: usize,
) -> Value {
    let (m, ret, params) = resolve_checked(family, mid, "A");
    // Fetch the jvalue words through the audited primitive — null args
    // with a demanding descriptor pends NPE (Temurin's is UB there).
    let mut cooked: Vec<Value> = Vec::with_capacity(params.len());
    if !params.is_empty() {
        if args_ptr == 0 {
            return pend_and_zero(
                &ret,
                crate::throw_new("java/lang/NullPointerException", None),
            );
        }
        let bytes = j2r_jni_ffi::ffi::copy_from_caller(args_ptr as *const u8, params.len() * 8);
        for (i, t) in params.iter().enumerate() {
            cooked.push(decode_jvalue(&bytes[i * 8..i * 8 + 8], t));
        }
    }
    dispatch(family, &m, &ret, cooked, receiver_tag, declared_tag)
}
// ---------------------------------------------------------------------
// The V-form walk (T11 slice 11c — docs/M5_JNI_CALLS.md §3 decision 2).
//
// On x86-64 SysV a `va_list` is a POINTER to one 24-byte `__va_list_tag`
// record: gp_offset @0, fp_offset @4, overflow_arg_area @8,
// reg_save_area @16. Consuming a vararg is the ABI's deterministic
// algorithm over that record — implemented here in SAFE code reading
// ONLY through the audited primitives (read_u32_at / read_word_at /
// read_fp_word_at; gate-10 count amended in the same commit).
//
// v1 law: the record is consumed into a LOCAL cursor — the caller's
// record is left untouched. A one-shot consume per Call*MethodV; a
// foreign caller that needs the args twice va_copy's first (standard C
// practice), and never mutating foreign memory is the conservative
// shape for a bridge.
//
// Classification per descriptor letter: F/D are SSE-class (fp_offset,
// 16-byte XMM slots, value in the low 8 bytes); every other kind is
// GP-class (gp_offset, 8-byte slots). Register bounds: GP < 48 (6
// slots), FP < 176 (48 + 8 slots × 16); beyond the bound the word comes
// from the overflow (stack) area, advanced 8 bytes per word.
const VFORM_GP_LIMIT: usize = 48;
const VFORM_FP_LIMIT: usize = 176;
/// Decodes the V-form arguments: walks the `__va_list_tag` record at
/// `ap` through the audited primitives and marshals each word per its
/// parameter descriptor. `Err(msg)` = an unreadable record/area — the
/// caller pends a loud internal error (never a silent mis-marshal).
/// A null `ap` with NO demanding parameters is fine (nothing to read);
/// the caller checks that case before calling here.
fn decode_vform_args(params: &[TypeDesc], ap: usize) -> Result<Vec<Value>, String> {
    use j2r_jni_ffi::ffi::{read_fp_word_at, read_u32_at, read_word_at};
    let p = ap as *const u8; // fed to the primitives only — no deref here
    let mut gp = read_u32_at(p).ok_or("unreadable gp_offset")? as usize;
    let mut fp = read_u32_at(p.wrapping_add(4)).ok_or("unreadable fp_offset")? as usize;
    let mut ovf = read_word_at(p.wrapping_add(8)).ok_or("unreadable overflow_arg_area")? as usize;
    let save = read_word_at(p.wrapping_add(16)).ok_or("unreadable reg_save_area")? as usize;
    let mut cooked = Vec::with_capacity(params.len());
    for t in params {
        let word: u64 = match t {
            TypeDesc::Float | TypeDesc::Double => {
                if fp < VFORM_FP_LIMIT {
                    let addr = save + fp;
                    fp += 16;
                    read_fp_word_at(addr as *const u8)
                } else {
                    let addr = ovf;
                    ovf += 8;
                    read_word_at(addr as *const u8)
                }
            }
            _ => {
                if gp < VFORM_GP_LIMIT {
                    let addr = save + gp;
                    gp += 8;
                    read_word_at(addr as *const u8)
                } else {
                    let addr = ovf;
                    ovf += 8;
                    read_word_at(addr as *const u8)
                }
            }
        }
        .ok_or_else(|| format!("unreadable va_list slot for param {t:?}"))?;
        cooked.push(decode_jvalue(&word.to_le_bytes(), t));
    }
    Ok(cooked)
}
/// The V-form invoke core (`Call*MethodV`) — same resolve/dispatch laws
/// as [`invoke_a`], the args come from the va_list walk instead of the
/// jvalue array. An unreadable record pends a loud internal error; a
/// null `ap` with a demanding descriptor pends NPE (Temurin's is UB
/// there — same shape as the A-form null-args law).
pub(crate) fn invoke_v(
    family: CallFamily,
    mid: u64,
    receiver_tag: u64,
    declared_tag: u64,
    ap: usize,
) -> Value {
    let (m, ret, params) = resolve_checked(family, mid, "V");
    let cooked = if params.is_empty() {
        Vec::new()
    } else {
        if ap == 0 {
            return pend_and_zero(
                &ret,
                crate::throw_new("java/lang/NullPointerException", None),
            );
        }
        match decode_vform_args(&params, ap) {
            Ok(c) => c,
            Err(msg) => {
                return pend_and_zero(
                    &ret,
                    internal_error(&format!("Call*MethodV for {}::{}: {msg}", m.owner, m.name)),
                );
            }
        }
    };
    dispatch(family, &m, &ret, cooked, receiver_tag, declared_tag)
}
