public class SwitchFallthrough {
    static String classify(int v) {
        String out = "";
        switch (v) {
            case 1:
                out += "one-";
            case 2:
                out += "two-";
                break;
            case 3:
            case 4:
                out += "three-or-four-";
                break;
            default:
                out += "other-";
            case 5:
                out += "five";
                break;
        }
        return out;
    }
    static byte small(byte b) {
        switch (b) {
            case 0: return 'z';
            case 1: return 'o';
            default: return '?';
        }
    }
    public static void main(String[] args) {
        System.out.println(classify(1));
        System.out.println(classify(2));
        System.out.println(classify(3));
        System.out.println(classify(4));
        System.out.println(classify(5));
        System.out.println(classify(9));
        System.out.println(small((byte) 0));
        System.out.println(small((byte) 1));
        System.out.println(small((byte) 7));
        // switch inside a loop with continue
        for (int i = 0; i < 6; i++) {
            switch (i % 3) {
                case 0:
                    System.out.println("m" + i);
                    continue;
                case 1:
                    System.out.println("p" + i);
                    break;
                default:
                    System.out.println("q" + i);
            }
            System.out.println("after" + i);
        }
    }
}
