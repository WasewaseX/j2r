/**
 * @name Direct stdout printing in the runtime and codegen crates
 * @description Generated J2R programs must reproduce the JVM's stdout
 *              byte-for-byte (differential gates 3/9). Any direct
 *              `println!`/`print!` inside j2r-runtime (linked into every
 *              generated binary) or j2r-codegen (owns the emission
 *              templates) pollutes that stream and breaks differential
 *              exactness. All Java-visible output must flow through the
 *              sanctioned stdout sink in j2r-runtime natives; diagnostics
 *              belong on stderr (`eprintln!`, which is permitted and not
 *              flagged).
 *
 *              The j2r-cli driver crate is intentionally out of scope:
 *              the compiler binary's own stdout is not part of the
 *              differential surface.
 * @kind problem
 * @problem.severity error
 * @id j2r/stdout-print
 */

import rust

class StdoutPrintMacro extends MacroCall {
  StdoutPrintMacro() {
    this
      .getLocation()
      .getFile()
      .getRelativePath()
      .regexpMatch("crates/(j2r-runtime|j2r-codegen)/src/.*") and
    this.getPath().getSegment().getIdentifier().getText().regexpMatch("(println|print)!?")
  }
}

from StdoutPrintMacro m
select m,
  "Direct stdout print macro: use eprintln! for diagnostics or the sanctioned stdout sink in j2r-runtime."
