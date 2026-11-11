/**
 * G4.2 arm (b) fixture — the JMM boundary enforcement gate's input.
 *
 * Carries every boundary shape the checker must catch, plus the two
 * negatives it must NOT flag:
 *   - `sflag`: ACC_VOLATILE static   -> boundary (volatile)
 *   - `flag`:  ACC_VOLATILE instance -> boundary (volatile)
 *   - `fixed`: ACC_FINAL instance (no ConstantValue) -> boundary (final)
 *   - `K`:     ACC_FINAL static WITH ConstantValue -> folded, NO boundary
 *   - `plain`: no flags -> NO boundary
 *
 * The gate compiles this file with javac, runs j2r compile on the
 * .class files, then scripts/check_jmm_boundary.py must pass against
 * the emitted crate. The main() output is also compared three-channel
 * (Temurin vs the j2r native binary) so the marker plumbing provably
 * did not change behavior: expected "52/3/7/42".
 */
public class JmmBoundary {
    static volatile int sflag = 7;
    volatile int flag;
    final int fixed;
    static final int K = 42;
    int plain;

    JmmBoundary(int v) {
        fixed = v;
        flag = v;
    }

    public static void main(String[] a) {
        JmmBoundary o = new JmmBoundary(3);
        o.flag = sflag;
        o.plain = o.flag + o.fixed + K;
        System.out.println(o.plain + "/" + o.fixed + "/" + sflag + "/" + K);
    }
}
