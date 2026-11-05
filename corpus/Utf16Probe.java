import java.io.UnsupportedEncodingException;

/**
 * Temurin vector for the UTF-16 charset family (the JZlib-wave loud gap):
 * getBytes/new String with UTF-16 (BOM + big-endian default), UTF-16BE,
 * UTF-16LE, surrogate roundtrips, invalid-input replacement, and the exact
 * UnsupportedEncodingException name for unmodelled charsets.
 */
public class Utf16Probe {

    static void hex(byte[] b) {
        StringBuilder sb = new StringBuilder();
        for (byte x : b) {
            // Integer.toHexString, not String.format — the Formatter engine
            // is a registered loud gap
            sb.append(Integer.toHexString((x & 0xff) + 0x100).substring(1));
        }
        System.out.println(sb);
    }

    public static void main(String[] args) throws UnsupportedEncodingException {
        // 1. UTF-16 default: BE bytes + explicit BOM prefix (FE FF)
        hex("A".getBytes("UTF-16"));      // FE FF 00 41
        hex("\u4E2D".getBytes("UTF-16")); // FE FF 4E 2D

        // 2. BE / LE without BOM
        hex("A".getBytes("UTF-16BE"));    // 00 41
        hex("A".getBytes("UTF-16LE"));    // 41 00
        hex("\u4E2D".getBytes("UTF-16BE")); // 4E 2D
        hex("\u4E2D".getBytes("UTF-16LE")); // 2D 4E

        // 3. astral plane: surrogate pair roundtrip (U+1D512)
        String astral = "\uD834\uDD12";
        hex(astral.getBytes("UTF-16BE")); // D8 34 DD 12
        hex(astral.getBytes("UTF-16LE")); // 34 D8 12 DD

        // 4. decode: BOM consumed, no-BOM defaults to BE
        byte[] bom = new byte[]{(byte) 0xFE, (byte) 0xFF, 0x00, 0x41};
        System.out.println(new String(bom, "UTF-16"));
        byte[] noBom = new byte[]{0x00, 0x41};
        System.out.println(new String(noBom, "UTF-16"));
        byte[] le = new byte[]{0x41, 0x00};
        System.out.println(new String(le, "UTF-16"));

        // 5. unpaired surrogate encodes as the replacement char U+FFFD
        //    (JDK 21: 0xFFFD bytes, NOT '?')
        String bad = "\uD834";
        hex(bad.getBytes("UTF-16BE")); // FF FD
        hex(bad.getBytes("UTF-16"));   // FE FF FF FD

        // 6. decode with an odd trailing byte -> U+FFFD replacement
        byte[] odd = new byte[]{0x00, 0x41, 0x00};
        System.out.println((int) new String(odd, "UTF-16").charAt(1));

        // 7. FF FE BOM switches UTF-16 decode to LITTLE-endian
        byte[] leBom = new byte[]{(byte) 0xFF, (byte) 0xFE, 0x41, 0x00};
        System.out.println(new String(leBom, "UTF-16"));

        // 8. a truly unknown charset name -> UEE whose message is the name
        //    (NOTE: UTF-32/KOI8_R are standard JDK charsets — they SUCCEED
        //    silently on Temurin; the standard provider registry beyond the
        //    UTF-16 family stays a registered loud gap in j2r)
        try {
            "A".getBytes("MADE-UP-CHARSET");
        } catch (UnsupportedEncodingException e) {
            System.out.println("UEE: " + e.getMessage());
        }
    }
}
