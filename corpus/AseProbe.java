public class AseProbe {
    static class Inner {}
    public static void main(String[] a) {
        // 1. wrong ref class into a class-typed array
        try {
            Object[] o = new String[1];
            o[0] = new Object();
        } catch (ArrayStoreException e) { System.out.println("1: " + e.getMessage()); }
        // 2. primitive-array rows into a ref array (descriptor-shaped name)
        try {
            Object[] o = new String[2][];
            o[0] = new int[3][3];
        } catch (ArrayStoreException e) { System.out.println("2: " + e.getMessage()); }
        // 3. a leaf int[] into int[][]-typed... (legal) — instead: String into int[][] rows
        try {
            Object[] o = new int[2][];   // rows are int[]
            o[0] = "x";
        } catch (ArrayStoreException e) { System.out.println("3: " + e.getMessage()); }
        // 4. user nested class name shape
        try {
            Object[] o = new String[1];
            o[0] = new Inner();
        } catch (ArrayStoreException e) { System.out.println("4: " + e.getMessage()); }
        // 5. super/subclass shapes: Object[] holding Integer[] rows, store a Long
        try {
            Object[] o = new Integer[1];
            o[0] = Long.valueOf(3);
        } catch (ArrayStoreException e) { System.out.println("5: " + e.getMessage()); }
    }
}
