# Test vectors — provenance

All files here were produced by the **real JDK** (Temurin 21.0.13,
`java.util.zip`) on 2026-11-02 and committed so the zero-dependency
inflate/zip unit tests never need a JDK at test time
(M3_PLAN §4.6 risk M3-R1).

| file | producer |
| --- | --- |
| `deflate_payload.bin` | 7 681 B structured text; the shared uncompressed input |
| `deflate_l0.bin` | raw DEFLATE (`new Deflater(0, true)`), stored blocks |
| `deflate_l1.bin` | raw DEFLATE (`new Deflater(1, true)`), fast Huffman |
| `deflate_l9.bin` | raw DEFLATE (`new Deflater(9, true)`), max Huffman |
| `jdk_l9.zip` | `ZipOutputStream`, two DEFLATED entries (`hello.txt`, `big.txt`) |
| `big_plain.txt` | the exact `big.txt` content, for round-trip comparison |

Regenerate only with a JDK and re-run the full suite afterwards; the
differential fixtures (`crates/j2r-cli/tests/suite_jar/`) are built by
the harness `jar` tool at test time, not from these files.
