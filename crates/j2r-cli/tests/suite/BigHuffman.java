/**
 * BigHuffman — real-program-scale differential fixture: Huffman coding
 * with a hand-rolled binary min-heap, bit-level encode/decode round trip,
 * and a run-length pre/post pass.
 *
 * Deterministic by construction: the corpus is a fixed string, the heap
 * tie-breaks on (frequency, insertion id), and no RNG or wall clock is
 * anywhere in the program.
 */
public class BigHuffman {

    // ------------------------------------------------------------------
    // Min-heap of nodes (array-backed, (freq, id) tie-break)
    // ------------------------------------------------------------------

    static final class Heap {
        final Node[] a;
        int size;

        Heap(int cap) {
            a = new Node[cap];
            size = 0;
        }

        boolean less(Node x, Node y) {
            if (x.freq != y.freq) {
                return x.freq < y.freq;
            }
            return x.id < y.id;
        }

        void push(Node n) {
            int i = size++;
            a[i] = n;
            while (i > 0) {
                int p = (i - 1) / 2;
                if (less(a[i], a[p])) {
                    Node t = a[p];
                    a[p] = a[i];
                    a[i] = t;
                    i = p;
                } else {
                    break;
                }
            }
        }

        Node pop() {
            Node top = a[0];
            size--;
            a[0] = a[size];
            a[size] = null;
            int i = 0;
            while (true) {
                int l = 2 * i + 1;
                int r = l + 1;
                int m = i;
                if (l < size && less(a[l], a[m])) {
                    m = l;
                }
                if (r < size && less(a[r], a[m])) {
                    m = r;
                }
                if (m == i) {
                    break;
                }
                Node t = a[m];
                a[m] = a[i];
                a[i] = t;
                i = m;
            }
            return top;
        }
    }

    static final class Node {
        final int id;
        final int freq;
        final int ch;        // -1 for internal nodes
        final Node left;
        final Node right;

        Node(int id, int freq, int ch) {
            this.id = id;
            this.freq = freq;
            this.ch = ch;
            this.left = null;
            this.right = null;
        }

        Node(int id, int freq, Node left, Node right) {
            this.id = id;
            this.freq = freq;
            this.ch = -1;
            this.left = left;
            this.right = right;
        }

        boolean isLeaf() {
            return left == null;
        }
    }

    // ------------------------------------------------------------------
    // Code table + encode/decode
    // ------------------------------------------------------------------

    static final String[] codeOf = new String[256];

    static void buildCodes(Node n, String prefix) {
        if (n.isLeaf()) {
            codeOf[n.ch] = prefix.length() == 0 ? "0" : prefix;
            return;
        }
        buildCodes(n.left, prefix + "0");
        buildCodes(n.right, prefix + "1");
    }

    static Node buildTree(int[] freq) {
        int nextId = 0;
        Heap h = new Heap(600);
        for (int c = 0; c < 256; c++) {
            if (freq[c] > 0) {
                h.push(new Node(nextId++, freq[c], c));
            }
        }
        if (h.size == 0) {
            return null;
        }
        // A single-symbol corpus still yields a usable (degenerate) tree.
        if (h.size == 1) {
            return h.pop();
        }
        while (h.size > 1) {
            Node a = h.pop();
            Node b = h.pop();
            h.push(new Node(nextId++, a.freq + b.freq, a, b));
        }
        return h.pop();
    }

    static String encode(String text) {
        StringBuilder bits = new StringBuilder();
        for (int i = 0; i < text.length(); i++) {
            bits.append(codeOf[text.charAt(i)]);
        }
        return bits.toString();
    }

    static String decode(String bits, Node root) {
        if (root.isLeaf()) {
            // Degenerate single-symbol tree: every bit (there is exactly
            // one code, "0") maps to the same char.
            int n = bits.length();
            StringBuilder sb = new StringBuilder();
            for (int i = 0; i < n; i++) {
                sb.append((char) root.ch);
            }
            return sb.toString();
        }
        StringBuilder out = new StringBuilder();
        Node cur = root;
        for (int i = 0; i < bits.length(); i++) {
            cur = bits.charAt(i) == '0' ? cur.left : cur.right;
            if (cur.isLeaf()) {
                out.append((char) cur.ch);
                cur = root;
            }
        }
        if (cur != root) {
            throw new IllegalStateException("dangling bits at end of stream");
        }
        return out.toString();
    }

    // ------------------------------------------------------------------
    // Run-length layer (on chars, independent of Huffman)
    // ------------------------------------------------------------------

    static String rleEncode(String s) {
        StringBuilder sb = new StringBuilder();
        int i = 0;
        while (i < s.length()) {
            char c = s.charAt(i);
            int run = 1;
            while (i + run < s.length() && s.charAt(i + run) == c && run < 9) {
                run++;
            }
            if (c == '\u0001') {
                // Literal sentinel: doubled on the wire, so it survives.
                sb.append('\u0001').append('\u0001');
                i++;
            } else if (run >= 3) {
                sb.append('\u0001').append(c).append((char) ('0' + run));
                i += run;
            } else {
                for (int k = 0; k < run; k++) {
                    sb.append(c);
                }
                i += run;
            }
        }
        return sb.toString();
    }

    static String rleDecode(String s) {
        StringBuilder sb = new StringBuilder();
        int i = 0;
        while (i < s.length()) {
            char c = s.charAt(i);
            if (c == '\u0001') {
                if (s.charAt(i + 1) == '\u0001') {
                    sb.append('\u0001');
                    i += 2;
                } else {
                    char val = s.charAt(i + 1);
                    int run = s.charAt(i + 2) - '0';
                    for (int k = 0; k < run; k++) {
                        sb.append(val);
                    }
                    i += 3;
                }
            } else {
                sb.append(c);
                i++;
            }
        }
        return sb.toString();
    }

    // ------------------------------------------------------------------
    // Driver
    // ------------------------------------------------------------------

    static int[] frequencies(String text) {
        int[] freq = new int[256];
        for (int i = 0; i < text.length(); i++) {
            freq[text.charAt(i)]++;
        }
        return freq;
    }

    static int symbolCount(int[] freq) {
        int n = 0;
        for (int c = 0; c < 256; c++) {
            if (freq[c] > 0) {
                n++;
            }
        }
        return n;
    }

    static String pipeline(String text) {
        for (int c = 0; c < 256; c++) {
            codeOf[c] = null;
        }
        String rled = rleEncode(text);
        if (rled.length() == 0) {
            return "syms=0 raw=" + text.length() + " rle=0 bits=0 ratio=0%";
        }
        int[] freq = frequencies(rled);
        Node tree = buildTree(freq);
        buildCodes(tree, "");
        String bits = encode(rled);
        String decodedRled = decode(bits, tree);
        String back = rleDecode(decodedRled);
        if (!back.equals(text)) {
            throw new IllegalStateException("round-trip mismatch, len=" + text.length());
        }
        return "syms=" + symbolCount(freq)
            + " raw=" + text.length()
            + " rle=" + rled.length()
            + " bits=" + bits.length()
            + " ratio=" + (bits.length() * 100) / (text.length() * 8) + "%";
    }

    public static void main(String[] args) {
        // Corpus 1: normal prose with repetition.
        String prose = "the quick brown fox jumps over the lazy dog; "
            + "the dog barks, the fox runs, and the quick brown fox "
            + "jumps over the lazy dog again and again and again. ";
        System.out.println("prose  " + pipeline(prose));

        // Corpus 2: long single-symbol runs (RLE does the heavy lifting).
        StringBuilder runs = new StringBuilder();
        for (int i = 0; i < 40; i++) {
            for (int k = 0; k < i % 7 + 2; k++) {
                runs.append("abcdef".charAt(i % 6));
            }
            runs.append(' ');
        }
        System.out.println("runs   " + pipeline(runs.toString()));

        // Corpus 3: every byte 0..255 in a skewed pattern (2 copies of each
        // plus 8 copies of every 16th char).
        StringBuilder wide = new StringBuilder();
        for (int c = 0; c < 256; c++) {
            wide.append((char) c);
            wide.append((char) c);
            if (c % 16 == 0) {
                for (int k = 0; k < 8; k++) {
                    wide.append((char) c);
                }
            }
        }
        System.out.println("wide   " + pipeline(wide.toString()));

        // Corpus 4: single symbol repeated (degenerate tree path).
        StringBuilder one = new StringBuilder();
        for (int i = 0; i < 100; i++) {
            one.append('x');
        }
        System.out.println("one    " + pipeline(one.toString()));

        // Corpus 5: empty input (edge case: zero symbols).
        System.out.println("empty  " + pipeline(""));

        // Code table sanity for the prose corpus: prefix-free property.
        for (int c = 0; c < 256; c++) {
            codeOf[c] = null;
        }
        int[] freq = frequencies(rleEncode(prose));
        Node tree = buildTree(freq);
        buildCodes(tree, "");
        int used = 0;
        for (int c = 0; c < 256; c++) {
            if (codeOf[c] != null) {
                used++;
                for (int d = 0; d < 256; d++) {
                    if (d != c && codeOf[d] != null) {
                        if (codeOf[d].startsWith(codeOf[c])) {
                            throw new IllegalStateException(
                                "code for " + c + " prefixes code for " + d);
                        }
                    }
                }
            }
        }
        System.out.println("prefix-free over " + used + " symbols: ok");

        // Weighted total: sum of freq*len must equal bit count.
        String rled = rleEncode(prose);
        int weighted = 0;
        for (int i = 0; i < rled.length(); i++) {
            weighted += codeOf[rled.charAt(i)].length();
        }
        System.out.println("weighted=" + weighted + " bits=" + encode(rled).length());

        System.out.println("BigHuffman done");
    }
}
