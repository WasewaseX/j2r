#!/usr/bin/env python3
"""M5 T01 scale-census farm generator (docs/M5_PLAN.md §2).

Emits a deterministic synthetic Java class farm: N classes in package
`farm`, each extending Base, implementing Op, holding fields/methods with
arithmetic, string concat, arrays and statics, and referencing 8 sibling
farm classes (5 forward-biased, 2 backward, 1 long jump — all mod N so the
whole web is one component rooted at C00000 and reachable from Main).
The reference shape stresses the classpath-closure worklist the way a
large library does: package-local webs plus long cross-package jumps.

Usage: m5_farm_gen.py N OUT_DIR   (writes OUT_DIR/farm_src/sources.txt too)
"""
import os
import sys


def refs(i: int, n: int):
    """8 deterministic sibling references for class i."""
    return [
        (i + 1) % n,
        (i + 3) % n,
        (i + 7) % n,
        (i + 15) % n,
        (i + 31) % n,
        (i - 3) % n,
        (i - 8) % n,
        (i + 1024) % n,
    ]


def gen_class(i: int, n: int) -> str:
    name = f"C{i:05d}"
    r = refs(i, n)
    k = (i * 2654435761) & 0x7FFFFFFF
    frac = k % 97 / 8.0
    fields = [
        "  private final int seed;",
        "  private double acc;",
        "  protected int[] buf;",
        "  private static int hits;",
    ]
    ctor = f"""  public {name}(int seed) {{
    super(seed);
    this.seed = seed;
    this.buf = new int[(seed & 31) + 8];
    for (int i = 0; i < this.buf.length; i++) {{
      this.buf[i] = seed + i * {k % 7 + 1};
    }}
    this.acc = 0.5 * seed + {frac:.3f};
  }}"""
    methods = [
        f"""  public int id() {{
    return {i};
  }}""",
        f"""  public double compute() {{
    double s = acc;
    for (int i = 0; i < buf.length; i++) {{
      s += buf[i] * 0.25 + (buf[i] & {k % 9 + 1});
    }}
    s += Base.gstate;
    Base.bump((long) (s * {k % 13 + 2}));
    hits++;
    s = s * 1.5 + 0.25;
    return s;
  }}""",
        f"""  public int mix(String t) {{
    return (t + "#{name}:" + seed).hashCode() ^ (int) compute() ^ hits;
  }}""",
        f"""  public static int shared(int x) {{
    return x * {k % 17 + 3} + {i % 100};
  }}""",
        f"""  public String tag() {{
    return "{name}:" + seed + ":" + acc + ":" + Base.gstate;
  }}""",
    ]
    # closure-pressure method: one construction + static calls per sibling
    calls = [f"    r += {f'C{j:05d}'}.shared(seed + {d});" for d, j in enumerate(r[:5])]
    calls.append(f"    r += (int) new {f'C{r[5]:05d}'}(seed + 11).compute();")
    calls.append(f'    r += new {f"C{r[6]:05d}"}(seed + 12).mix("w6");')
    calls.append(f"    r += {f'C{r[7]:05d}'}.shared(seed + 13);")
    methods.append(
        "  public int call() {\n    int r = seed;\n" + "\n".join(calls) + "\n    return r;\n  }"
    )
    body = "\n".join(fields + [""] + [ctor] + [""] + ["\n\n".join(methods)])
    return f"""package farm;

/** Farm class {name} (census T01; deterministic generator). */
public class {name} extends Base implements Op {{
{body}
}}
"""


def gen_main(n: int) -> str:
    step = max(1, n // 200)
    direct = list(range(0, n, step))[:512]
    builds = "\n".join(
        f"      sum += new C{i:05d}({i}).compute();" for i in direct
    )
    calls = "\n".join(
        f"      chk += new C{i:05d}({i}).call();" for i in direct[:128]
    )
    return f"""package farm;

/** Census driver: exercises a deterministic subset of the farm. */
public class Main {{
  public static void main(String[] args) {{
    double sum = 0;
    long chk = 0;
    Base.gstate = 7;
{builds}
{calls}
    System.out.println("classes={n}");
    System.out.println("sum=" + sum);
    System.out.println("chk=" + chk);
    System.out.println("gstate=" + Base.gstate);
  }}
}}
"""


def gen_base() -> str:
    return """package farm;

/** Farm shared base (census T01). */
public abstract class Base {
  protected final int seed0;
  public static long gstate;

  protected Base(int seed) {
    this.seed0 = seed;
  }

  protected int baseMix(int x) {
    return (x ^ seed0) * 31 + (int) (gstate & 1023);
  }

  public static void bump(long v) {
    gstate += v;
  }
}
"""


def gen_op() -> str:
    return """package farm;

/** Farm interface (census T01). */
public interface Op {
  int id();
  double compute();
  int mix(String t);
  int call();
}
"""


def main() -> None:
    n = int(sys.argv[1])
    out = sys.argv[2]
    srcdir = os.path.join(out, "farm_src")
    os.makedirs(os.path.join(srcdir, "farm"), exist_ok=True)
    sources = []
    with open(os.path.join(srcdir, "farm", "Op.java"), "w") as f:
        f.write(gen_op())
        sources.append("farm/Op.java")
    with open(os.path.join(srcdir, "farm", "Base.java"), "w") as f:
        f.write(gen_base())
        sources.append("farm/Base.java")
    for i in range(n):
        with open(os.path.join(srcdir, "farm", f"C{i:05d}.java"), "w") as f:
            f.write(gen_class(i, n))
        sources.append(f"farm/C{i:05d}.java")
    with open(os.path.join(srcdir, "farm", "Main.java"), "w") as f:
        f.write(gen_main(n))
        sources.append("farm/Main.java")
    with open(os.path.join(srcdir, "sources.txt"), "w") as f:
        f.write("\n".join(sources) + "\n")
    total_loc = 0
    for s in sources:
        with open(os.path.join(srcdir, s)) as f:
            total_loc += sum(1 for _ in f)
    print(f"farm: {n} classes + Base + Op + Main, {total_loc} total LOC -> {srcdir}")


if __name__ == "__main__":
    main()
