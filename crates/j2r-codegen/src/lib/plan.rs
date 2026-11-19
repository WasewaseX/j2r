//! `lib`::plan — the verbatim
//! items of the round-7 file-split pass (the parent glob
//! re-exports preserve every pre-split path).
use super::*;
use j2r_classfile::ClassFile;
use j2r_runtime::builtin;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn to_internal(name: &str) -> String {
    name.replace('.', "/")
}

pub(crate) fn builtin_is_instantiable(class: &str) -> bool {
    if matches!(
        class,
        "java/lang/Object"
            | "java/lang/String"
            | "java/lang/StringBuilder"
            | "java/lang/StringBuffer"
            // M3 Wave 2: collections are heap-resident runtime classes
            // (M3_PLAN §5.1/§5.4 — the layout pass skips them; their slots
            // are owned by j2r-runtime/src/collections.rs)
            | "java/util/ArrayList"
            | "java/util/HashMap"
            | "java/util/HashSet"
            // run 4 (2026-11-15): LinkedHashMap — the ordered map (the
            // HashMap subclass shape; slots owned by collections.rs)
            | "java/util/LinkedHashMap"
            | "java/util/LinkedHashMap$KeySet"
            | "java/util/LinkedHashMap$Values"
            | "java/util/LinkedHashMap$EntrySet"
            | "java/util/LinkedHashMap$KeyIterator"
            | "java/util/LinkedHashMap$ValueIterator"
            | "java/util/LinkedHashMap$EntryIterator"
            // run 5 (2026-11-15): TreeMap — the sorted map (the red-black
            // storage; slots owned by collections.rs)
            | "java/util/TreeMap"
            | "java/util/TreeMap$KeySet"
            | "java/util/TreeMap$Values"
            | "java/util/TreeMap$EntrySet"
            | "java/util/TreeMap$KeyIterator"
            | "java/util/TreeMap$ValueIterator"
            | "java/util/TreeMap$EntryIterator"
            // stringsim corpus wave (2026-11-07): entrySet views, the
            // unmodifiableMap wrapper, and Sift4's LinkedList — slots
            // owned by j2r-runtime/src/collections.rs
            | "java/util/HashMap$EntrySet"
            | "java/util/HashMap$EntryIterator"
            | "java/util/Collections$UnmodifiableMap"
            | "java/util/LinkedList"
            // corpus wave 1 (F3): java.io StringWriter/StringReader —
            // payloads managed by j2r-runtime/src/io.rs; the layout pass
            // skips builtins
            | "java/io/StringWriter"
            | "java/io/StringReader"
            // M4 Wave 2 (T06): java.lang.Thread — slots owned by
            // j2r-runtime/src/thread.rs (runnable/name/daemon/started/tid);
            // the layout pass skips builtins.
            | "java/lang/Thread"
            // Corpus wave 2 (JZlib): ByteArrayOutputStream — JDK field model
            // in slots (buf byte[] ref, count Int), behavior in io.rs.
            | "java/io/ByteArrayOutputStream"
            // Skip/charset wave: ByteArrayInputStream — slots
            // (buf ref, pos, mark, count), behavior in io.rs; semantics
            // pinned by corpus/SkipProbe.java (close() no-op, negative skip
            // = 0, skip past EOF = actual count).
            | "java/io/ByteArrayInputStream"
            // Stream wave 3: BufferedInputStream — slots (inherited in,
            // buf ref, pos, count, markpos, marklimit), behavior in io.rs;
            // pinned by corpus/BufferedProbe.java ("Stream closed" family,
            // mark/readlimit invalidation, the skip buffer-vs-inner law).
            | "java/io/BufferedInputStream"
            // Stream wave 4: PushbackInputStream — slots (inherited in,
            // pushback buf ref, pos), behavior in io.rs; pinned by
            // corpus/PushbackProbe.java.
            | "java/io/PushbackInputStream"
            // Corpus wave 3 (JAMA): PrintWriter (real subset, natives.rs),
            // plus the registered-loud trio (Vector / StreamTokenizer /
            // DecimalFormat family — constructors resolve, behavior natives
            // throw naming the gap).
            | "java/io/PrintWriter"
            | "java/util/Vector"
            | "java/io/StreamTokenizer"
            | "java/text/DecimalFormat"
            | "java/text/DecimalFormatSymbols"
            // EXTERNAL-B6 (jbox2d port, 2026-11-13): the 8 boxed classes
            // — JDK-deprecated-but-alive constructors (jbox2d emits
            // `new Integer(int)` in World.<clinit>). Fresh boxes: the
            // <init> natives set the payload slot without the valueOf
            // identity cache (new Integer(127) == new Integer(127) is
            // FALSE on HotSpot; see the NATIVES B6 block).
            | "java/lang/Integer"
            | "java/lang/Long"
            | "java/lang/Double"
            | "java/lang/Float"
            | "java/lang/Boolean"
            | "java/lang/Character"
            | "java/lang/Short"
            | "java/lang/Byte"
            // EXTERNAL-B10 (jbox2d port): java.util.Random — a real
            // builtin object with per-instance state (seed in slot 0,
            // gaussian cache in slots 1-2; behavior natives in natives.rs).
            | "java/util/Random"
    ) {
        return true;
    }
    // any builtin Throwable subclass is instantiable (its <init> is native)
    let mut cur = class;
    for _ in 0..16 {
        match builtin::builtin_parent_of(cur) {
            Some(p) => {
                if p == "java/lang/Throwable" {
                    return true;
                }
                cur = p;
            }
            None => return false,
        }
    }
    false
}

// ---------------------------------------------------------------- compilation

/// Parse one `.class` file and append it to the program universe.
/// Shared by explicit inputs (M1) and classpath closure loading (M3
/// Wave 1) — identical validation for both sources.
pub(crate) fn parse_and_add(
    bytes: &[u8],
    classes: &mut Vec<ClassPlan>,
    user_index: &mut BTreeMap<String, usize>,
    static_slots: &mut u32,
    referer_of: &mut BTreeMap<String, String>,
    module_index: &mut BTreeMap<String, usize>,
    source: &str,
) -> Result<()> {
    use crate::sha256::sha256_hex;
    let cf = ClassFile::parse(bytes).map_err(cf_err)?;
    let name = cf.this_class.clone();
    if classpath::is_jdk_namespace(&name) {
        return Err(CodegenError::Semantic {
            class: name.clone(),
            message: "refusing to compile a JDK builtin class; it is provided by j2r-runtime"
                .to_string(),
        });
    }
    if user_index.contains_key(&name) {
        return Err(CodegenError::Semantic {
            class: name.clone(),
            message: format!("duplicate class {name} (loaded twice — from {source})"),
        });
    }
    let module = sanitize_module(&name);
    // T03 closure cache: extract the referenced-user-class set once, here,
    // so the closure worklist never re-walks a constant pool.
    let refs = classpath::referenced_user_classes(&cf);
    // G2.3 (F1): the module-collision check indexes instead of scanning —
    // the old `classes.iter().any(...)` was the second O(N²) term in the
    // measured 20k stage1 profile. Same semantics: duplicate module →
    // loud error naming the colliding class.
    if module_index.contains_key(&module) {
        return Err(CodegenError::Semantic {
            class: name.clone(),
            message: format!("module name collision for {name} (two classes sanitize to {module})"),
        });
    }
    let is_interface = cf.is_interface();
    let is_abstract = cf.is_abstract() || is_interface;
    let id = classes.len() as u32;

    // fields: layout + static slots
    let mut fields = Vec::new();
    let mut instance_count = 0usize;
    for f in &cf.fields {
        let is_static = f.is_static();
        let (static_slot, instance_slot) = if is_static {
            let s = *static_slots;
            *static_slots += 1;
            (Some(s), None)
        } else {
            let s = instance_count;
            instance_count += 1;
            (None, Some(s))
        };
        fields.push(FieldPlan {
            name: f.name.clone(),
            desc: f.descriptor.clone(),
            is_static,
            static_slot,
            instance_slot,
            constant_value: f.constant_value.clone(),
            is_volatile: f.is_volatile(),
            // final + ConstantValue is folded (JMM-safe); only the
            // non-folded finals carry the boundary.
            is_final: f.is_final() && f.constant_value.is_none(),
            modifiers: f.access,
            signature: f.signature.clone(),
        });
    }

    user_index.insert(name.clone(), classes.len());
    module_index.insert(module.clone(), classes.len());
    // G2.3 (F1): the closure worklist's referer attribution — a class's
    // FIRST referer in class order, exactly what the old linear
    // `classes.iter().find(...)` scan returned, but built in O(refs) here
    // instead of O(N × refs) per wanted name (the measured 20k
    // super-linearity: 26.09 s of the 29.93 s no-op emit).
    for ref_name in &refs {
        referer_of
            .entry(ref_name.clone())
            .or_insert_with(|| name.clone());
    }
    classes.push(ClassPlan {
        name: name.clone(),
        super_name: cf.super_class.clone(),
        interfaces: cf.interfaces.clone(),
        is_interface,
        is_abstract,
        simple_name: cf.inner_name.clone(),
        nest_host: cf.nest_host.clone(),
        module,
        id,
        base_slots: 0,
        total_slots: 0,
        default_spec: String::new(),
        fields,
        methods: Vec::new(),
        clinit_fn: None,
        concats: BTreeMap::new(),
        lambdas: BTreeMap::new(),
        static_monitor: None,
        refs,
        input_sha: sha256_hex(bytes),
        cf,
    });
    Ok(())
}

pub fn compile(opts: &Options) -> Result<Program> {
    let mut prog = compile_unchecked(opts)?;
    // 5b. verify + 6. validate every instruction --------------------------
    // (the full pipeline's hard gate: malformed bytecode must fail here
    // with a structured diagnostic, not reach the panic paths of the
    // generated code or runtime. The surface report walks the program
    // BEFORE this gate — that is its entire reason to exist.)
    let _sp_verify = crate::StageGuard::enter("stage4_verify_validate");
    for c in &prog.classes {
        verify::verify_class(&prog, c)?;
    }
    // G3.3 slice 2+3: the specialized-map cores. Runs AFTER verify (the
    // receiver walk consumes the verifier's per-pc shapes) and BEFORE
    // validate/emit (the synthetic instructions carry no symbolic refs
    // and both emitters verify them again with their own arms). Slice 3:
    // per class, the static-field scan first proves the getstatic
    // receiver set (every PutStatic a HashMap creation run, every
    // GetStatic a guarded core's receiver with the field signature's
    // pinned prim); the per-method pass then rewrites both receiver
    // kinds.
    let mut map_cores_rewritten = 0usize;
    for c in &mut prog.classes {
        let proven = crate::peephole::proven_map_fields(c);
        for m in &mut c.methods {
            let Ok(shapes) = verify::verify_method_shapes(m, &c.concats, &c.lambdas) else {
                continue; // verified OK above; the rewrite never fires on a refuse
            };
            map_cores_rewritten += crate::peephole::map_prim_rewrites(
                &mut m.instrs,
                &m.exception_table,
                &shapes,
                &proven,
            );
        }
    }
    let _ = map_cores_rewritten;
    for c in &prog.classes {
        validate_class(&prog, c)?;
    }

    // 7. main class ---------------------------------------------------------
    let main_class = match &opts.main_class {
        Some(m) => {
            let internal = to_internal(m);
            if !prog.is_user(&internal) {
                return Err(CodegenError::Semantic {
                    class: internal.clone(),
                    message: "--main-class not found among the inputs".to_string(),
                });
            }
            let plan = prog.class(&internal).unwrap();
            if plan.method("main", "([Ljava/lang/String;)V").is_none() {
                return Err(CodegenError::Semantic {
                    class: internal.clone(),
                    message: "--main-class has no public static void main(String[])".to_string(),
                });
            }
            Some(internal)
        }
        None => {
            // Programmatic default only (first declared main in input
            // order). The `j2r test` differential path (audit A3) now
            // always passes an explicit --main-class selected once for
            // BOTH sides, so the JVM run and the native run cannot pick
            // different mains.
            let found = prog.classes.iter().find(|c| {
                c.method("main", "([Ljava/lang/String;)V")
                    .map(|m| m.is_static)
                    .unwrap_or(false)
            });
            found.map(|c| c.name.clone())
        }
    };
    prog.main_class = main_class;

    Ok(prog)
}

/// The classpath closure phase (verbatim from `compile_unchecked` section
/// 1b, round-5 reshaping): seed the walk from --main-class, then the
/// transitive CONSTANT_Class closure (M3 Wave 1, M3_PLAN §4.2).
#[allow(clippy::type_complexity)]
pub(crate) fn classpath_closure(
    opts: &Options,
    classes: &mut Vec<ClassPlan>,
    user_index: &mut BTreeMap<String, usize>,
    static_slots: &mut u32,
    referer_of: &mut BTreeMap<String, String>,
    module_index: &mut BTreeMap<String, usize>,
) -> Result<()> {
    if opts.classpath.is_empty() {
        return Ok(());
    }
    let cp = classpath::Classpath::new(&opts.classpath).map_err(|e| CodegenError::Semantic {
        class: "-".to_string(),
        message: e.message,
    })?;

    // Seed: with no explicit inputs the main class starts the walk.
    if opts.inputs.is_empty() {
        let main = opts
            .main_class
            .as_deref()
            .ok_or_else(|| CodegenError::Semantic {
                class: "-".to_string(),
                message: "compiling from a classpath requires --main-class (nothing else seeds the closure)"
                    .to_string(),
            })?;
        let main = to_internal(main);
        let src = cp.find(&main).ok_or_else(|| CodegenError::Semantic {
            class: main.clone(),
            message: format!("main class {main} not found on the classpath"),
        })?;
        let bytes = cp.read(&src).map_err(|e| CodegenError::Semantic {
            class: main.clone(),
            message: e.message,
        })?;
        parse_and_add(
            &bytes,
            classes,
            user_index,
            static_slots,
            referer_of,
            module_index,
            &format!("classpath source {src}"),
        )?;
    }

    // Transitive closure over CONSTANT_Class references.
    let mut processed = 0usize;
    loop {
        let mut wanted: BTreeSet<String> = BTreeSet::new();
        for c in classes.iter() {
            // T03: cached at parse time — no constant-pool re-walk here.
            for name in &c.refs {
                if !user_index.contains_key(name) {
                    wanted.insert(name.clone());
                }
            }
        }
        if wanted.is_empty() {
            break;
        }
        for name in wanted {
            // G2.3 (F1): the referer map (built at parse time) replaces
            // the old linear `classes.iter().find(...)` scan — the
            // measured super-linear term at 20k. Same semantics: the
            // FIRST referer in class order, diagnostics-only.
            let referer = referer_of
                .get(&name)
                .cloned()
                .unwrap_or_else(|| "<unknown>".to_string());
            let src = cp.find(&name).ok_or_else(|| CodegenError::Semantic {
                class: referer.clone(),
                message: format!(
                    "class {name} is referenced but not found on the classpath (referenced by {referer})"
                ),
            })?;
            let bytes = cp.read(&src).map_err(|e| CodegenError::Semantic {
                class: referer.clone(),
                message: e.message,
            })?;
            parse_and_add(
                &bytes,
                classes,
                user_index,
                static_slots,
                referer_of,
                module_index,
                &format!("classpath source {src}"),
            )?;
        }
        processed += 1;
        if processed > 100_000 {
            return Err(CodegenError::Semantic {
                class: "-".to_string(),
                message: "classpath closure did not converge (internal bug)".to_string(),
            });
        }
    }
    Ok(())
}

/// Superclass/interface validation (verbatim from `compile_unchecked`
/// section 2, round-5 reshaping).
pub(crate) fn validate_superclasses(prog: &Program) -> Result<()> {
    for c in &prog.classes {
        if let Some(sup) = &c.super_name {
            if !prog.is_user(sup)
                && sup != "java/lang/Object"
                && builtin::builtin_parent_of(sup).is_none()
            {
                return Err(CodegenError::Semantic {
                    class: c.name.clone(),
                    message: format!("unknown superclass {sup}: add its .class file to the inputs"),
                });
            }
            let sup_plan = prog.class(sup);
            let sup_is_interface = sup_plan.map(|s| s.is_interface).unwrap_or(false);
            if !c.is_interface && sup_is_interface {
                return Err(CodegenError::Semantic {
                    class: c.name.clone(),
                    message: format!("class cannot extend interface {sup}"),
                });
            }
        }
        for itf in &c.interfaces {
            if !prog.is_user(itf) {
                // builtin interfaces are tolerated (Object/Comparable/...)
                continue;
            }
            if !prog.class(itf).map(|x| x.is_interface).unwrap_or(false) {
                return Err(CodegenError::Semantic {
                    class: c.name.clone(),
                    message: format!("interface {itf} is not an interface"),
                });
            }
        }
    }
    Ok(())
}

/// Decode + synthesize clinit + the G2.6 array-literal collapse (verbatim
/// from `compile_unchecked` section 4, round-5 reshaping).
pub(crate) fn plan_all_methods(prog: &mut Program) -> Result<()> {
    let mut static_monitor_count: u32 = 0;
    for ci in 0..prog.classes.len() {
        let (methods, clinit_fn, concats, lambdas) = plan_methods(prog, ci)?;
        let plan = &mut prog.classes[ci];
        plan.methods = methods;
        plan.clinit_fn = clinit_fn;
        plan.concats = concats;
        plan.lambdas = lambdas;
        // G2.6 (console.md F6, REVIEW_CHECKLIST §G2.6 arm a): collapse
        // javac's array-literal clinit fill runs into ONE literal-array
        // instruction BEFORE verify/emit, so the emitted clinit is one
        // constructor call over a const-evaluated static instead of 4N
        // interpreter statements (the measured MIR/borrow-check wall:
        // InfTree <clinit> was 12,354 emitted lines). Byte-identical by
        // construction; every refused shape keeps the old form.
        for m in &mut plan.methods {
            crate::peephole::collapse_array_fill_runs(&mut m.instrs);
        }
        if plan.methods.iter().any(|m| m.is_sync && m.is_static) {
            plan.static_monitor = Some(static_monitor_count);
            static_monitor_count += 1;
        }
    }
    Ok(())
}

/// The parse/plan pipeline WITHOUT the verify+validate hard gate — the
/// surface report's entry point (EXTERNAL-HARNESS H-1/H-2): the whole
/// question is "what WOULD fail", so the walk must see past the first
/// unsupported shape. Emission must always go through `compile`.
pub fn compile_unchecked(opts: &Options) -> Result<Program> {
    if opts.inputs.is_empty() && opts.classpath.is_empty() {
        return Err(CodegenError::Semantic {
            class: "-".to_string(),
            message: "no input .class files and no classpath".to_string(),
        });
    }

    // 1. parse ---------------------------------------------------------
    let _sp_load = crate::StageGuard::enter("stage1_load_parse_hash");
    let mut classes: Vec<ClassPlan> = Vec::new();
    let mut user_index: BTreeMap<String, usize> = BTreeMap::new();
    let mut static_slots = 0u32;
    // G2.3: wanted-name -> first referer (class order), built at parse
    // time; replaces the closure loop's O(N × refs) linear attribution.
    let mut referer_of: BTreeMap<String, String> = BTreeMap::new();
    let mut module_index: BTreeMap<String, usize> = BTreeMap::new();

    for path in &opts.inputs {
        let bytes = std::fs::read(path).map_err(CodegenError::Io)?;
        parse_and_add(
            &bytes,
            &mut classes,
            &mut user_index,
            &mut static_slots,
            &mut referer_of,
            &mut module_index,
            &format!("input file {}", path.display()),
        )?;
    }

    // 1b. classpath closure (M3 Wave 1, M3_PLAN §4.2) ------------------
    classpath_closure(
        opts,
        &mut classes,
        &mut user_index,
        &mut static_slots,
        &mut referer_of,
        &mut module_index,
    )?;
    drop(_sp_load);

    let mut prog = Program {
        classes,
        user_index,
        total_statics: static_slots as usize,
        main_class: None,
    };

    // 2. superclass validation -------------------------------------------
    validate_superclasses(&prog)?;

    // 3. slot layouts ------------------------------------------------------
    compute_layouts(&mut prog)?;

    // 4. decode + synthesize clinit ----------------------------------------
    let _sp_plan = crate::StageGuard::enter("stage3_plan_methods_peephole");
    plan_all_methods(&mut prog)?;
    drop(_sp_plan);

    // 5. verify every method body (before emitting any code) ---------------
    // (verify + validate + the main-class gate moved to `compile` — the
    // surface walk deliberately sees past them)

    Ok(prog)
}
