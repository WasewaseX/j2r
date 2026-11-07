/**
 * Differential fixture: the java.lang.Math semantics wave.
 *
 * Pins the edge laws that "natural" Rust ports get wrong (the silent-semantic
 * class of the external java-to-rust report):
 *
 *   1. Math.round(double/float) is the MATHEMATICAL floor(a + 1/2) — computed
 *      by OpenJDK's bit trick since JDK-8010430 (Java 7), NOT floor(a + 0.5)
 *      evaluated in floating point. The canonical divergence:
 *      round(0.49999999999999994) is 0 on the JDK (a + 0.5 rounds up to 1.0
 *      in binary FP, but a is still below one half), while a naive FP
 *      implementation returns 1. The same law holds at every binade:
 *      round(4503599627370497.0) is 4503599627370497 (a mathematical integer
 *      returned through the (long) cast), not ...498.
 *   2. Math.min/max keep Java's signed-zero law: min(+0.0, -0.0) is -0.0,
 *      max(-0.0, +0.0) is +0.0 — bit-visible via doubleToLongBits, which
 *      preserves signed zeros. NaN results are compared through
 *      doubleToLongBits/floatToIntBits, which canonicalize every NaN —
 *      the raw payload of a NaN returned by min/max is a C2-intrinsic
 *      artifact on HotSpot (max(NaN,1.0) returns 1.0's bits via maxsd on
 *      x86-64 while min returns the NaN operand) and is deliberately NOT
 *      pinned; NaN-ness is.
 *   3. Math.abs: wrapping on int/long (abs(MIN_VALUE) is MIN_VALUE), and
 *      abs(-0.0) is +0.0 (bit-visible).
 *
 * Every row is deterministic; NaN comparisons go through the canonicalizing
 * bit-conversion calls, so the fixture is byte-stable across VMs.
 */
public class MathSemantics {
    public static void main(String[] args) {
        // ---- Math.round(double): the JDK-8010430 class ------------------
        System.out.println("rd_just_below_half=" + Math.round(0.49999999999999994));
        System.out.println("rd_just_below_half_neg=" + Math.round(-0.49999999999999994));
        System.out.println("rd_ulp_below_half=" + Math.round(0.4999999999999999));
        System.out.println("rd_half=" + Math.round(0.5));
        System.out.println("rd_half_neg=" + Math.round(-0.5));
        System.out.println("rd_2_5=" + Math.round(2.5));
        System.out.println("rd_3_5=" + Math.round(3.5));
        System.out.println("rd_2_5_neg=" + Math.round(-2.5));
        System.out.println("rd_8_5=" + Math.round(8.5));
        System.out.println("rd_8_5_neg=" + Math.round(-8.5));
        System.out.println("rd_just_above_2p52_int=" + Math.round(4503599627370497.0));
        System.out.println("rd_2p52_minus_half=" + Math.round(4503599627370495.5));
        System.out.println("rd_2p53_plus_1=" + Math.round(9007199254740993.0));
        System.out.println("rd_1e18=" + Math.round(1.0e18));
        System.out.println("rd_2p63=" + Math.round(9.223372036854776E18));
        System.out.println("rd_beyond_2p63=" + Math.round(1.0e19));
        System.out.println("rd_neg_2p63=" + Math.round(-9.223372036854776E18));
        System.out.println("rd_neg_beyond=" + Math.round(-1.0e19));
        System.out.println("rd_nan=" + Math.round(Double.NaN));
        System.out.println("rd_pos_inf=" + Math.round(Double.POSITIVE_INFINITY));
        System.out.println("rd_neg_inf=" + Math.round(Double.NEGATIVE_INFINITY));
        System.out.println("rd_zero=" + Math.round(0.0));
        System.out.println("rd_zero_neg=" + Math.round(-0.0));

        // ---- Math.round(float): the same law at f32 precision -----------
        System.out.println("rf_just_below_half=" + Math.round(0.49999997f));
        System.out.println("rf_just_below_half_neg=" + Math.round(-0.49999997f));
        System.out.println("rf_half=" + Math.round(0.5f));
        System.out.println("rf_half_neg=" + Math.round(-0.5f));
        System.out.println("rf_2_5=" + Math.round(2.5f));
        System.out.println("rf_2_5_neg=" + Math.round(-2.5f));
        System.out.println("rf_just_above_2p23_int=" + Math.round(8388609.0f));
        System.out.println("rf_2p23_minus_half=" + Math.round(8388607.5f));
        System.out.println("rf_max_as_float=" + Math.round(2.14748365E9f));
        System.out.println("rf_beyond_int=" + Math.round(3.0e9f));
        System.out.println("rf_neg_beyond=" + Math.round(-3.0e9f));
        System.out.println("rf_nan=" + Math.round(Float.NaN));
        System.out.println("rf_pos_inf=" + Math.round(Float.POSITIVE_INFINITY));
        System.out.println("rf_neg_inf=" + Math.round(Float.NEGATIVE_INFINITY));
        System.out.println("rf_zero_neg=" + Math.round(-0.0f));

        // ---- min/max: the signed-zero law, bit-visible ------------------
        System.out.println("dmin_pz_nz=" + Double.doubleToLongBits(Math.min(0.0, -0.0)));
        System.out.println("dmin_nz_pz=" + Double.doubleToLongBits(Math.min(-0.0, 0.0)));
        System.out.println("dmax_pz_nz=" + Double.doubleToLongBits(Math.max(0.0, -0.0)));
        System.out.println("dmax_nz_pz=" + Double.doubleToLongBits(Math.max(-0.0, 0.0)));
        System.out.println("fmin_pz_nz=" + Float.floatToIntBits(Math.min(0.0f, -0.0f)));
        System.out.println("fmax_nz_pz=" + Float.floatToIntBits(Math.max(-0.0f, 0.0f)));

        // ---- min/max: NaN-ness (canonicalized contract) -----------------
        System.out.println("dmax_nan_right=" + Double.doubleToLongBits(Math.max(1.0, Double.NaN)));
        System.out.println("dmax_nan_left=" + Double.doubleToLongBits(Math.max(Double.NaN, 1.0)));
        System.out.println("dmin_nan_right=" + Double.doubleToLongBits(Math.min(1.0, Double.NaN)));
        System.out.println("dmin_nan_nan=" + Double.doubleToLongBits(Math.min(Double.NaN, Double.NaN)));
        System.out.println("fmin_nan_right=" + Float.floatToIntBits(Math.min(1.0f, Float.NaN)));

        // ---- min/max: ordinary ordering, all four arities ---------------
        System.out.println("imax=" + Math.max(3, -7));
        System.out.println("imin=" + Math.min(3, -7));
        System.out.println("jmax=" + Math.max(-9L, 12L));
        System.out.println("jmin=" + Math.min(-9L, 12L));
        System.out.println("dmax_ord=" + Math.max(2.25, 2.5));
        System.out.println("dmin_ord=" + Math.min(2.25, 2.5));
        System.out.println("fmax_ord=" + Math.max(1.5f, 1.25f));
        System.out.println("fmin_ord=" + Math.min(1.5f, 1.25f));
        System.out.println("dmax_eq=" + Math.max(4.5, 4.5));
        System.out.println("dmin_eq=" + Math.min(4.5, 4.5));

        // ---- abs: wrapping integers, signed zero ------------------------
        System.out.println("iabs_min=" + Math.abs(Integer.MIN_VALUE));
        System.out.println("jabs_min=" + Math.abs(Long.MIN_VALUE));
        System.out.println("iabs=" + Math.abs(-42));
        System.out.println("jabs=" + Math.abs(-42L));
        System.out.println("dabs_nz=" + Double.doubleToLongBits(Math.abs(-0.0)));
        System.out.println("dabs_min=" + Math.abs(-Double.MIN_VALUE));
        System.out.println("fabs_nz=" + Float.floatToIntBits(Math.abs(-0.0f)));
        System.out.println("dabs_nan=" + Double.doubleToLongBits(Math.abs(Double.NaN)));
    }
}
