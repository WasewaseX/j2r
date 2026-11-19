use super::*;

/// Java `Integer.toUnsignedString(x, 2|8|16)` — unsigned formatting.
macro_rules! int_radix_fns {
    ($bin:ident, $oct:ident, $hex:ident, $t:ty) => {
        pub fn $bin(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let v = args.first().cloned().unwrap_or(Value::Int(0));
            let u = unsigned_of(&v);
            Ok(Value::Ref(Some(crate::object::new_string(&format!(
                "{:b}",
                u
            )))))
        }
        pub fn $oct(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let v = args.first().cloned().unwrap_or(Value::Int(0));
            let u = unsigned_of(&v);
            Ok(Value::Ref(Some(crate::object::new_string(&format!(
                "{:o}",
                u
            )))))
        }
        pub fn $hex(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let v = args.first().cloned().unwrap_or(Value::Int(0));
            let u = unsigned_of(&v);
            Ok(Value::Ref(Some(crate::object::new_string(&format!(
                "{:x}",
                u
            )))))
        }
    };
}

fn unsigned_of(v: &Value) -> u64 {
    match v {
        Value::Int(i) => *i as u32 as u64,
        Value::Long(l) => *l as u64,
        other => panic!("j2r internal: expected int/long, got {other:?}"),
    }
}

int_radix_fns!(n_int_to_binary, n_int_to_octal, n_int_to_hex, i32);
int_radix_fns!(n_long_to_binary, n_long_to_octal, n_long_to_hex, i64);
pub fn n_int_compare(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let a = i32_at(args, 0, "Integer.compare");
    let b = i32_at(args, 1, "Integer.compare");
    Ok(Value::Int(if a < b {
        -1
    } else if a > b {
        1
    } else {
        0
    }))
}
pub fn n_long_compare(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let a = arg(args, 0, "Long.compare").expect_long();
    let b = arg(args, 1, "Long.compare").expect_long();
    Ok(Value::Int(if a < b {
        -1
    } else if a > b {
        1
    } else {
        0
    }))
}

macro_rules! int_min_max_sum {
    ($min:ident, $max:ident, $sum:ident, $t:ty, $wrap:ident) => {
        pub fn $min(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let a = num_at::<$t>(args, 0);
            let b = num_at::<$t>(args, 1);
            Ok(make_num::<$t>(a.min(b)))
        }
        pub fn $max(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let a = num_at::<$t>(args, 0);
            let b = num_at::<$t>(args, 1);
            Ok(make_num::<$t>(a.max(b)))
        }
        pub fn $sum(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
            let a = num_at::<$t>(args, 0);
            let b = num_at::<$t>(args, 1);
            Ok(make_num::<$t>(a.$wrap(b)))
        }
    };
}
fn num_at<T>(args: &[Value], i: usize) -> T
where
    Value: ExtractNum<T>,
{
    args.get(i)
        .unwrap_or_else(|| panic!("j2r internal: missing arg {i}"))
        .extract()
}
fn make_num<T>(v: T) -> Value
where
    Value: ExtractNum<T>,
{
    Value::from_num(v)
}
/// Trait glue so the min/max/sum helpers work for both i32 and i64.
pub trait ExtractNum<T> {
    fn extract(&self) -> T;
    fn from_num(v: T) -> Value;
}
impl ExtractNum<i32> for Value {
    fn extract(&self) -> i32 {
        self.expect_int()
    }
    fn from_num(v: i32) -> Value {
        Value::Int(v)
    }
}
impl ExtractNum<i64> for Value {
    fn extract(&self) -> i64 {
        self.expect_long()
    }
    fn from_num(v: i64) -> Value {
        Value::Long(v)
    }
}

int_min_max_sum!(n_int_min, n_int_max, n_int_sum, i32, wrapping_add);
int_min_max_sum!(n_long_min, n_long_max, n_long_sum, i64, wrapping_add);
pub fn n_int_bit_count(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.bitCount");
    Ok(Value::Int(v.count_ones() as i32))
}
pub fn n_long_bit_count(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.bitCount").expect_long();
    Ok(Value::Int(v.count_ones() as i32))
}
pub fn n_int_nlz(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.numberOfLeadingZeros");
    Ok(Value::Int(v.leading_zeros() as i32))
}
pub fn n_int_ntz(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.numberOfTrailingZeros");
    Ok(Value::Int(v.trailing_zeros() as i32))
}
pub fn n_long_nlz(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.numberOfLeadingZeros").expect_long();
    Ok(Value::Int(v.leading_zeros() as i32))
}
pub fn n_long_ntz(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.numberOfTrailingZeros").expect_long();
    Ok(Value::Int(v.trailing_zeros() as i32))
}
pub fn n_int_highest_one_bit(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.highestOneBit");
    let out = if v == 0 {
        0
    } else {
        1i32.wrapping_shl(31 - (v as u32).leading_zeros())
    };
    Ok(Value::Int(out))
}
pub fn n_int_lowest_one_bit(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.lowestOneBit");
    Ok(Value::Int(v.isolate_lowest_one()))
}
pub fn n_long_highest_one_bit(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.highestOneBit").expect_long();
    let out = if v == 0 {
        0
    } else {
        1i64.wrapping_shl(63 - (v as u64).leading_zeros())
    };
    Ok(Value::Long(out))
}
pub fn n_long_lowest_one_bit(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.lowestOneBit").expect_long();
    Ok(Value::Long(v.isolate_lowest_one()))
}
pub fn n_int_rotate_left(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.rotateLeft");
    let d = i32_at(args, 1, "Integer.rotateLeft");
    Ok(Value::Int(v.rotate_left((d as u32) & 31)))
}
pub fn n_int_rotate_right(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.rotateRight");
    let d = i32_at(args, 1, "Integer.rotateRight");
    Ok(Value::Int(v.rotate_right((d as u32) & 31)))
}
pub fn n_long_rotate_left(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.rotateLeft").expect_long();
    let d = i32_at(args, 1, "Long.rotateLeft");
    Ok(Value::Long(v.rotate_left((d as u32) & 63)))
}
pub fn n_long_rotate_right(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.rotateRight").expect_long();
    let d = i32_at(args, 1, "Long.rotateRight");
    Ok(Value::Long(v.rotate_right((d as u32) & 63)))
}
pub fn n_int_reverse(_recv: Option<&JRef>, args: &[Value], _ctx: Dispatch) -> Result<Value, Throw> {
    let v = i32_at(args, 0, "Integer.reverse");
    Ok(Value::Int(v.reverse_bits()))
}
pub fn n_long_reverse(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let v = arg(args, 0, "Long.reverse").expect_long();
    Ok(Value::Long(v.reverse_bits()))
}
/// `Integer.parseInt(String, int radix)` (the no-radix form exists since M1).
pub fn n_int_parse_int_radix(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    let s = match arg(args, 0, "Integer.parseInt") {
        Value::Ref(Some(r)) => crate::object::string_value(r).to_string(),
        _ => {
            return Err(throw_new(
                "java/lang/NumberFormatException",
                Some("Cannot parse null string"),
            ))
        }
    };
    let radix = i32_at(args, 1, "Integer.parseInt");
    if !(CharacterRanges::MIN_RADIX..=36).contains(&radix) {
        return Err(throw_new(
            "java/lang/NumberFormatException",
            Some(&format!("radix {radix} out of range")),
        ));
    }
    parse_signed(&s, radix as u32, "Integer.parseInt")
}
/// Shared signed parse with Java's exact NumberFormatException messages.
pub fn parse_signed(s: &str, radix: u32, who: &str) -> Result<Value, Throw> {
    // JDK 17+: `For input string: "X"` with a ` under radix R` suffix when
    // the radix is not 10 (forInputString(s, radix))
    let nfe = move |text: &str| {
        let suffix = if radix == 10 {
            String::new()
        } else {
            format!(" under radix {radix}")
        };
        throw_new(
            "java/lang/NumberFormatException",
            Some(&format!("For input string: \"{text}\"{suffix}")),
        )
    };
    if s.is_empty() {
        return Err(nfe(s));
    }
    let (digits, negative) = match s.as_bytes()[0] {
        b'-' | b'+' => (&s[1..], s.as_bytes()[0] == b'-'),
        _ => (s, false),
    };
    if digits.is_empty() {
        return Err(nfe(s));
    }
    let mut acc: i64 = 0;
    for c in digits.chars() {
        let d = match c.to_digit(36) {
            Some(d) if d < radix => d as i64,
            _ => return Err(nfe(s)),
        };
        acc = acc
            .checked_mul(radix as i64)
            .and_then(|x| x.checked_add(d))
            .ok_or_else(|| nfe(s))?;
    }
    let limit = if negative {
        -(i32::MIN as i64)
    } else {
        i32::MAX as i64
    };
    if acc > limit {
        return Err(nfe(s));
    }
    let v = if negative { -acc } else { acc };
    let _ = who;
    Ok(Value::Int(v as i32))
}

// ---------------------------------------------------------------- Character
