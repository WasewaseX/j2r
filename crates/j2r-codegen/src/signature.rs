//! The generic-signature grammar (JVMS §4.7.9.1) — the G3.3 signature-pin
//! layer.
//!
//! The erased descriptor of `Map<String, Double> m` says
//! `Ljava/util/Map;`; only the classfile's generic signatures (the
//! `Signature` attribute on classes/fields/methods and the
//! `LocalVariableTypeTable` entries in `Code`) carry the type arguments.
//! The specialized-map rewrites (register §5 step 3, the 2.78x container
//! tax) pin a map's value type from exactly those strings: when the pin
//! says the value parameter is a boxed primitive, the codegen may rewrite
//! `valueOf→put` / `get→doubleValue` chains into the prim-slot natives —
//! only where a dataflow window proves the box never escapes (the
//! registered faithfulness guard, evidence g3_3/opening_census).
//!
//! This module parses the grammar into a small typed model. It is
//! deliberately conservative: anything it does not fully understand is a
//! loud error (the codebase's parser law), and the pin helpers below
//! accept ONLY exact, unbounded type arguments — a `Map<String, ? extends
//! Double>` or a subclass of `HashMap` that does not repeat its own
//! arguments yields NO pin, so the rewrite slices keep the faithful
//! boxing path there (registered widening follow-ons).

/// One type in a generic signature (JVMS §4.7.9.1 `JavaTypeSignature`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SigType {
    /// A base type `B C D F I J S Z`.
    Prim(char),
    /// A class type `L a/b/C <args> ;` — `name` is the binary name in
    /// internal form (`java/util/HashMap`), `args` the type arguments
    /// (empty for a raw use).
    Class { name: String, args: Vec<SigArg> },
    /// A type variable `T X ;`.
    TypeVar(String),
    /// An array type `[ JavaTypeSignature`.
    Array(Box<SigType>),
}

/// One type argument (JVMS §4.7.9.1 `TypeArgument`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SigArg {
    /// An exact reference type (no wildcard).
    In(SigType),
    /// `*` — an unconstrained wildcard.
    Star,
    /// `+ RefType` — an upper-bounded wildcard.
    Extends(SigType),
    /// `- RefType` — a lower-bounded wildcard.
    Super(SigType),
}

/// A parsed method signature: `[TypeParams] ( params ) ret {^throws}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodSig {
    /// Declared type-parameter names, in order (bounds parsed and dropped).
    pub type_params: Vec<String>,
    pub params: Vec<SigType>,
    /// `None` when the result is `V` (void).
    pub ret: Option<SigType>,
    pub throws: Vec<SigType>,
}

/// A parsed class signature: `[TypeParams] Superclass {Superinterface}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassSig {
    pub type_params: Vec<String>,
    pub superclass: SigType,
    pub interfaces: Vec<SigType>,
}

/// A boxed primitive class the prim-slot map path can represent: the JLS
/// cache law differs per class (`Double`/`Float` are NEVER cached;
/// `Integer`/`Long` are cached in [-128, 127]) — the rewrite slices must
/// respect that when a box could escape, so the identity of the box is
/// carried in the pin, not just the width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxedPrim {
    Double,
    Float,
    Integer,
    Long,
}

impl BoxedPrim {
    /// The boxed class's binary name (internal form).
    pub fn class_name(self) -> &'static str {
        match self {
            BoxedPrim::Double => "java/lang/Double",
            BoxedPrim::Float => "java/lang/Float",
            BoxedPrim::Integer => "java/lang/Integer",
            BoxedPrim::Long => "java/lang/Long",
        }
    }

    /// The descriptor char of the primitive payload.
    pub fn value_desc(self) -> char {
        match self {
            BoxedPrim::Double => 'D',
            BoxedPrim::Float => 'F',
            BoxedPrim::Integer => 'I',
            BoxedPrim::Long => 'J',
        }
    }

    /// The unboxing instance method javac calls after the `checkcast`
    /// (`(Double) m.get(k)).doubleValue()`).
    pub fn unbox_method(self) -> &'static str {
        match self {
            BoxedPrim::Double => "doubleValue",
            BoxedPrim::Float => "floatValue",
            BoxedPrim::Integer => "intValue",
            BoxedPrim::Long => "longValue",
        }
    }

    /// `Double`/`Float` from a class name — the never-cached pair, the
    /// G3.3 opening scope (`Double.valueOf` has no JLS cache, every
    /// autobox is a fresh object, so the dataflow guard is the ONLY
    /// identity hazard). `Integer`/`Long` are named here too: their
    /// [-128, 127] cache means a materialized box must come from the
    /// cache, a stricter law the rewrite slices must discharge before
    /// widening to them.
    pub fn boxed_prim_of(name: &str) -> Option<BoxedPrim> {
        match name {
            "java/lang/Double" => Some(BoxedPrim::Double),
            "java/lang/Float" => Some(BoxedPrim::Float),
            "java/lang/Integer" => Some(BoxedPrim::Integer),
            "java/lang/Long" => Some(BoxedPrim::Long),
            _ => None,
        }
    }
}

/// The map classes whose second type parameter is the VALUE type. The
/// runtime's implemented container today is `HashMap`; the family is
/// listed so a signature that names any of them pins uniformly (the
/// rewrite slices gate on which natives exist, not on this list).
const MAP_FAMILY: &[&str] = &[
    "java/util/Map",
    "java/util/AbstractMap",
    "java/util/HashMap",
    "java/util/LinkedHashMap",
    "java/util/TreeMap",
    "java/util/SortedMap",
    "java/util/NavigableMap",
    "java/util/Hashtable",
    "java/util/WeakHashMap",
    "java/util/IdentityHashMap",
    "java/util/concurrent/ConcurrentMap",
    "java/util/concurrent/ConcurrentHashMap",
    "java/util/concurrent/ConcurrentNavigableMap",
];

/// The G3.3 map pin: given a raw field/local generic signature, returns
/// the boxed-primitive VALUE type when the signature is exactly
/// `L <map-family> < K ; V ; > ;` with V an exact boxed-primitive class
/// (no wildcards, no nested args, no type variables). Everything else —
/// raw types, bounded wildcards, subtypes that do not repeat their
/// arguments — pins to `None`: no pin, no rewrite, the faithful boxing
/// path stands.
pub fn map_value_boxed(field_signature: &str) -> Option<BoxedPrim> {
    let SigType::Class { name, args } = parse_field_signature(field_signature).ok()? else {
        return None;
    };
    if !MAP_FAMILY.contains(&name.as_str()) || args.len() != 2 {
        return None;
    }
    let SigArg::In(SigType::Class {
        name: value,
        args: value_args,
    }) = &args[1]
    else {
        return None;
    };
    if !value_args.is_empty() {
        return None;
    }
    BoxedPrim::boxed_prim_of(value)
}

// ---------------------------------------------------------------------------
// The parser. A tiny cursor over the raw string; every failure is a loud
// error naming the byte offset and what was expected (the codebase's
// parser law — no silent truncation).
// ---------------------------------------------------------------------------

/// Characters that may not appear inside a signature identifier (the
/// grammar's meta characters; JVMS §4.7.9.1 identifiers are unqualified
/// names, which cannot contain any of these).
const ID_TERMINATORS: &[u8] = b".;[/<>:*+-^";

struct Cursor<'a> {
    s: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(s: &'a str) -> Cursor<'a> {
        Cursor {
            s: s.as_bytes(),
            pos: 0,
        }
    }

    fn err(&self, what: &str) -> String {
        format!("generic signature at byte {}: {what}", self.pos)
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.eat(c) {
            Ok(())
        } else {
            Err(self.err(&format!("expected '{}'", c as char)))
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.s.len()
    }

    fn expect_end(&self) -> Result<(), String> {
        if self.at_end() {
            Ok(())
        } else {
            Err(self.err(&format!(
                "expected end of signature, found '{}'",
                self.peek().unwrap_or(b'?') as char
            )))
        }
    }

    fn ident(&mut self) -> Result<String, String> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if ID_TERMINATORS.contains(&c) {
                break;
            }
            self.pos += 1;
        }
        if self.pos == start {
            return Err(self.err("expected an identifier"));
        }
        // Signature identifiers are ISO-8859-1 per JVMS §4.4.7 (the Utf8
        // form j2r accepts); ASCII-only byte-slicing is exact here.
        Ok(String::from_utf8_lossy(&self.s[start..self.pos]).into_owned())
    }

    /// `JavaTypeSignature`: a base type or a reference type.
    fn java_type(&mut self) -> Result<SigType, String> {
        match self.peek() {
            Some(c @ (b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z')) => {
                self.pos += 1;
                Ok(SigType::Prim(c as char))
            }
            Some(b'L' | b'T' | b'[') => self.ref_type(),
            _ => Err(self.err("expected a Java type signature")),
        }
    }

    /// `ReferenceTypeSignature`: class | type variable | array. A bare
    /// base type is rejected (it is not a reference type).
    fn ref_type(&mut self) -> Result<SigType, String> {
        match self.peek() {
            Some(b'L') => self.class_type(),
            Some(b'T') => {
                self.pos += 1;
                let id = self.ident()?;
                self.expect(b';')?;
                Ok(SigType::TypeVar(id))
            }
            Some(b'[') => {
                self.pos += 1;
                Ok(SigType::Array(Box::new(self.java_type()?)))
            }
            Some(c) => Err(self.err(&format!(
                "expected a reference type, found base type or '{}'",
                c as char
            ))),
            None => Err(self.err("expected a reference type, found end of signature")),
        }
    }

    /// `ClassTypeSignature`: `L PackageSpecifier* Identifier
    /// [TypeArguments] ;`. The spec's `.` nested-class suffix form is
    /// parsed but rejected loudly when it carries its own type arguments —
    /// javac emits the `$`-in-identifier form (`Ljava/util/Map$Entry;`)
    /// for everything j2r compiles today.
    fn class_type(&mut self) -> Result<SigType, String> {
        self.expect(b'L')?;
        let mut name = String::new();
        loop {
            name.push_str(&self.ident()?);
            if self.eat(b'/') {
                name.push('/');
            } else {
                break;
            }
        }
        let mut args = Vec::new();
        if self.eat(b'<') {
            while self.peek() != Some(b'>') {
                args.push(self.type_arg()?);
            }
            self.expect(b'>')?;
        }
        while self.eat(b'.') {
            let nested = self.ident()?;
            name.push('$');
            name.push_str(&nested);
            if self.peek() == Some(b'<') {
                return Err(self.err(
                    "nested-class suffix type arguments are not supported (javac emits \
                     the $ form)",
                ));
            }
        }
        self.expect(b';')?;
        Ok(SigType::Class { name, args })
    }

    /// `TypeArgument`: `[+|-] ReferenceTypeSignature | *`.
    fn type_arg(&mut self) -> Result<SigArg, String> {
        match self.peek() {
            Some(b'*') => {
                self.pos += 1;
                Ok(SigArg::Star)
            }
            Some(b'+') => {
                self.pos += 1;
                Ok(SigArg::Extends(self.ref_type()?))
            }
            Some(b'-') => {
                self.pos += 1;
                Ok(SigArg::Super(self.ref_type()?))
            }
            _ => Ok(SigArg::In(self.ref_type()?)),
        }
    }

    /// `TypeParametersSignature`: `< Ident ClassBound {InterfaceBound}+ >`.
    /// Bounds are parsed (the cursor must stay legal) and dropped — the
    /// pin layer needs only the parameter names.
    fn type_params(&mut self) -> Result<Vec<String>, String> {
        self.expect(b'<')?;
        let mut names = Vec::new();
        while self.peek() != Some(b'>') {
            names.push(self.ident()?);
            self.expect(b':')?;
            // ClassBound: `: [ReferenceTypeSignature]` — absent when the
            // next char is the second ':' of an interface bound.
            if self.peek() != Some(b':') {
                self.ref_type()?;
            }
            while self.eat(b':') {
                self.ref_type()?;
            }
        }
        self.expect(b'>')?;
        Ok(names)
    }
}

/// Parses a field signature (JVMS §4.7.9.1 `FieldSignature` = one
/// reference type). This is also the shape of a `LocalVariableTypeTable`
/// entry's signature.
pub fn parse_field_signature(s: &str) -> Result<SigType, String> {
    let mut c = Cursor::new(s);
    let t = c.ref_type()?;
    c.expect_end()?;
    Ok(t)
}

/// Parses a method signature (JVMS §4.7.9.1 `MethodSignature`).
pub fn parse_method_signature(s: &str) -> Result<MethodSig, String> {
    let mut c = Cursor::new(s);
    let type_params = if c.peek() == Some(b'<') {
        c.type_params()?
    } else {
        Vec::new()
    };
    c.expect(b'(')?;
    let mut params = Vec::new();
    while c.peek() != Some(b')') {
        params.push(c.java_type()?);
    }
    c.expect(b')')?;
    let ret = if c.eat(b'V') {
        None
    } else {
        Some(c.java_type()?)
    };
    let mut throws = Vec::new();
    while c.eat(b'^') {
        throws.push(c.ref_type()?);
    }
    c.expect_end()?;
    Ok(MethodSig {
        type_params,
        params,
        ret,
        throws,
    })
}

/// Parses a class signature (JVMS §4.7.9.1 `ClassSignature`):
/// `[TypeParams] SuperclassSignature {SuperinterfaceSignature}` — the
/// superclass and every superinterface are class type signatures.
pub fn parse_class_signature(s: &str) -> Result<ClassSig, String> {
    let mut c = Cursor::new(s);
    let type_params = if c.peek() == Some(b'<') {
        c.type_params()?
    } else {
        Vec::new()
    };
    let superclass = c.class_type()?;
    let mut interfaces = Vec::new();
    while !c.at_end() {
        interfaces.push(c.class_type()?);
    }
    Ok(ClassSig {
        type_params,
        superclass,
        interfaces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- the G3.3 pin, on the exact strings javac emits -----------------

    #[test]
    fn pins_map_string_double() {
        // The opening-census instrument's shape (BoxingBoxedBench).
        assert_eq!(
            map_value_boxed("Ljava/util/Map<Ljava/lang/String;Ljava/lang/Double;>;"),
            Some(BoxedPrim::Double)
        );
        assert_eq!(
            map_value_boxed("Ljava/util/HashMap<Ljava/lang/String;Ljava/lang/Double;>;"),
            Some(BoxedPrim::Double)
        );
        assert_eq!(
            map_value_boxed("Ljava/util/TreeMap<Ljava/lang/String;Ljava/lang/Double;>;"),
            Some(BoxedPrim::Double)
        );
    }

    #[test]
    fn pins_the_other_boxed_primitives() {
        assert_eq!(
            map_value_boxed("Ljava/util/Map<Ljava/lang/String;Ljava/lang/Integer;>;"),
            Some(BoxedPrim::Integer)
        );
        assert_eq!(
            map_value_boxed("Ljava/util/HashMap<Ljava/lang/String;Ljava/lang/Long;>;"),
            Some(BoxedPrim::Long)
        );
        assert_eq!(
            map_value_boxed("Ljava/util/HashMap<Ljava/lang/String;Ljava/lang/Float;>;"),
            Some(BoxedPrim::Float)
        );
    }

    #[test]
    fn refuses_non_prims_raw_and_wildcards() {
        // Value type String: no boxed prim.
        assert_eq!(
            map_value_boxed("Ljava/util/Map<Ljava/lang/String;Ljava/lang/String;>;"),
            None
        );
        // Raw Map (no type args).
        assert_eq!(map_value_boxed("Ljava/util/Map;"), None);
        // Bounded wildcard: conservative None (registered widening
        // follow-on — never rewrite on a pin we do not fully understand).
        assert_eq!(
            map_value_boxed("Ljava/util/Map<Ljava/lang/String;+Ljava/lang/Double;>;"),
            None
        );
        assert_eq!(
            map_value_boxed("Ljava/util/Map<Ljava/lang/String;*>;"),
            None
        );
        // A generic List is not a map.
        assert_eq!(
            map_value_boxed("Ljava/util/List<Ljava/lang/Double;>;"),
            None
        );
        // A generic-class value type: no pin.
        assert_eq!(
            map_value_boxed(
                "Ljava/util/HashMap<Ljava/lang/String;Ljava/util/List<Ljava/lang/Double;>;>;"
            ),
            None
        );
        // A type-variable value: no pin.
        assert_eq!(
            map_value_boxed("Ljava/util/Map<Ljava/lang/String;TV;>;"),
            None
        );
        // Not a signature at all.
        assert_eq!(map_value_boxed("garbage"), None);
    }

    // --- the grammar table ----------------------------------------------

    #[test]
    fn parses_field_shapes() {
        assert_eq!(
            parse_field_signature("Ljava/lang/Double;").unwrap(),
            SigType::Class {
                name: "java/lang/Double".into(),
                args: vec![]
            }
        );
        assert_eq!(
            parse_field_signature("[Ljava/lang/Double;").unwrap(),
            SigType::Array(Box::new(SigType::Class {
                name: "java/lang/Double".into(),
                args: vec![]
            }))
        );
        // Primitive arrays are legal inside array signatures.
        assert_eq!(
            parse_field_signature("[[I").unwrap(),
            SigType::Array(Box::new(SigType::Array(Box::new(SigType::Prim('I')))))
        );
        // Nested classes: javac's $ form is one identifier.
        assert_eq!(
            parse_field_signature("Ljava/util/Map$Entry;").unwrap(),
            SigType::Class {
                name: "java/util/Map$Entry".into(),
                args: vec![]
            }
        );
        // A type variable.
        assert_eq!(
            parse_field_signature("TT;").unwrap(),
            SigType::TypeVar("T".into())
        );
    }

    #[test]
    fn parses_type_arguments_and_wildcards() {
        // Three args: exact, star (a bare TypeArgument — NO trailing ';'),
        // lower-bounded.
        let t = parse_field_signature("Ljava/util/Map<Ljava/lang/String;*-Ljava/lang/Number;>;")
            .unwrap();
        let SigType::Class { name, args } = t else {
            panic!("expected class type");
        };
        assert_eq!(name, "java/util/Map");
        assert_eq!(args.len(), 3);
        assert_eq!(
            args[0],
            SigArg::In(SigType::Class {
                name: "java/lang/String".into(),
                args: vec![]
            })
        );
        assert_eq!(args[1], SigArg::Star);
        assert_eq!(
            args[2],
            SigArg::Super(SigType::Class {
                name: "java/lang/Number".into(),
                args: vec![]
            })
        );
        // Upper-bounded and type-variable args on a synthetic 2-arg class.
        let t2 = parse_field_signature("Lx/Y<+Ljava/lang/Number;+TT;>;").unwrap();
        let SigType::Class { args: args2, .. } = t2 else {
            panic!()
        };
        assert_eq!(
            args2[0],
            SigArg::Extends(SigType::Class {
                name: "java/lang/Number".into(),
                args: vec![]
            })
        );
        assert_eq!(args2[1], SigArg::Extends(SigType::TypeVar("T".into())));
    }

    #[test]
    fn parses_nested_generic_args() {
        let t = parse_field_signature("Ljava/util/Map<Ljava/lang/String;Ljava/util/List<[D>;>;")
            .unwrap();
        let SigType::Class { args, .. } = t else {
            panic!()
        };
        assert_eq!(
            args[1],
            SigArg::In(SigType::Class {
                name: "java/util/List".into(),
                args: vec![SigArg::In(SigType::Array(Box::new(SigType::Prim('D'))))]
            })
        );
    }

    #[test]
    fn parses_method_signature_shapes() {
        // A plain (Object)Object.
        let m = parse_method_signature("(Ljava/lang/Object;)Ljava/lang/Object;").unwrap();
        assert_eq!(m.type_params.len(), 0);
        assert_eq!(
            m.params,
            vec![SigType::Class {
                name: "java/lang/Object".into(),
                args: vec![]
            }]
        );
        assert_eq!(
            m.ret,
            Some(SigType::Class {
                name: "java/lang/Object".into(),
                args: vec![]
            })
        );
        assert!(m.throws.is_empty());

        // void run(int, double) with a throws clause.
        let m = parse_method_signature("(ID)V^Ljava/io/IOException;^TE;").unwrap();
        assert_eq!(m.params, vec![SigType::Prim('I'), SigType::Prim('D')]);
        assert_eq!(m.ret, None);
        assert_eq!(m.throws.len(), 2);
        assert_eq!(
            m.throws[0],
            SigType::Class {
                name: "java/io/IOException".into(),
                args: vec![]
            }
        );
        assert_eq!(m.throws[1], SigType::TypeVar("E".into()));

        // A generic method: <K, V extends Comparable<? super K>> (K)V —
        // Collections.sort's own shape.
        let m = parse_method_signature(
            "<K:Ljava/lang/Object;V:Ljava/lang/Object;:Ljava/lang/Comparable<-TK;>;>\
             (Ljava/util/List<TV;>;)V",
        )
        .unwrap();
        assert_eq!(m.type_params, vec!["K".to_string(), "V".to_string()]);
        assert_eq!(m.params.len(), 1);
        assert_eq!(
            m.params[0],
            SigType::Class {
                name: "java/util/List".into(),
                args: vec![SigArg::In(SigType::TypeVar("V".into()))]
            }
        );
        assert_eq!(m.ret, None);
    }

    #[test]
    fn parses_class_signature_shapes() {
        // HashMap<K,V> extends AbstractMap<K,V> implements Map<K,V> —
        // the JDK shape with repeated type variables.
        let c = parse_class_signature(
            "<K:Ljava/lang/Object;V:Ljava/lang/Object;>\
             Ljava/util/AbstractMap<TK;TV;>;Ljava/util/Map<TK;TV;>;",
        )
        .unwrap();
        assert_eq!(c.type_params, vec!["K".to_string(), "V".to_string()]);
        assert_eq!(
            c.superclass,
            SigType::Class {
                name: "java/util/AbstractMap".into(),
                args: vec![
                    SigArg::In(SigType::TypeVar("K".into())),
                    SigArg::In(SigType::TypeVar("V".into()))
                ]
            }
        );
        assert_eq!(
            c.interfaces,
            vec![SigType::Class {
                name: "java/util/Map".into(),
                args: vec![
                    SigArg::In(SigType::TypeVar("K".into())),
                    SigArg::In(SigType::TypeVar("V".into()))
                ]
            }]
        );

        // No type params: a plain class extends Object.
        let c = parse_class_signature("Ljava/lang/Object;").unwrap();
        assert!(c.type_params.is_empty());
        assert_eq!(
            c.superclass,
            SigType::Class {
                name: "java/lang/Object".into(),
                args: vec![]
            }
        );
        assert!(c.interfaces.is_empty());
    }

    #[test]
    fn errors_are_loud_and_positioned() {
        // A base type is not a reference type (a field signature can't be D).
        let e = parse_field_signature("D").unwrap_err();
        assert!(e.contains("reference type"), "{e}");
        // Truncated class type.
        let e = parse_field_signature("Ljava/util/Map<Ljava/lang/String;").unwrap_err();
        assert!(e.contains("at byte"), "{e}");
        // Trailing garbage.
        let e = parse_field_signature("Ljava/lang/Double;X").unwrap_err();
        assert!(e.contains("expected end"), "{e}");
        // Method signature missing the closing paren.
        assert!(parse_method_signature("(I)V").is_ok());
        assert!(parse_method_signature("(I").is_err());
        // Empty input.
        assert!(parse_field_signature("").is_err());
        // A nested '.' suffix with args is a loud unsupported error.
        assert!(parse_field_signature("Ljava/util/Map.Entry<TK;>;").is_err());
        // The '.' suffix WITHOUT args parses to the $ binary name.
        assert_eq!(
            parse_field_signature("Ljava/util/Map.Entry;").unwrap(),
            SigType::Class {
                name: "java/util/Map$Entry".into(),
                args: vec![]
            }
        );
    }

    #[test]
    fn boxed_prim_table_is_the_jdk_law() {
        assert_eq!(
            BoxedPrim::boxed_prim_of("java/lang/Double"),
            Some(BoxedPrim::Double)
        );
        assert_eq!(
            BoxedPrim::boxed_prim_of("java/lang/Float"),
            Some(BoxedPrim::Float)
        );
        assert_eq!(
            BoxedPrim::boxed_prim_of("java/lang/Integer"),
            Some(BoxedPrim::Integer)
        );
        assert_eq!(
            BoxedPrim::boxed_prim_of("java/lang/Long"),
            Some(BoxedPrim::Long)
        );
        // Subclass-looking or unrelated names: None (javac never subclasses
        // boxed classes; anything else must not pin).
        assert_eq!(BoxedPrim::boxed_prim_of("java/lang/Number"), None);
        assert_eq!(BoxedPrim::boxed_prim_of("java/lang/Double2"), None);
        // The unbox-method table matches the JDK's BoxedPrim class shape.
        assert_eq!(BoxedPrim::Double.unbox_method(), "doubleValue");
        assert_eq!(BoxedPrim::Float.unbox_method(), "floatValue");
        assert_eq!(BoxedPrim::Integer.unbox_method(), "intValue");
        assert_eq!(BoxedPrim::Long.unbox_method(), "longValue");
        // The descriptor chars match the primitive payloads.
        assert_eq!(BoxedPrim::Double.value_desc(), 'D');
        assert_eq!(BoxedPrim::Float.value_desc(), 'F');
        assert_eq!(BoxedPrim::Integer.value_desc(), 'I');
        assert_eq!(BoxedPrim::Long.value_desc(), 'J');
    }
}
