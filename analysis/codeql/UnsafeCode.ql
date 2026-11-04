/**
 * @name Unsafe Rust vectors (fn, impl, asm, unsafe attribute)
 * @description J2R is a safe-Rust-only project: every crate root carries
 *              `#![forbid(unsafe_code)]` and CI gate-10 greps for unsafe
 *              items. This query is a semantic second net for the four
 *              high-signal vectors: `unsafe fn`, `unsafe impl`, inline
 *              assembly, and `#[unsafe(...)]` attributes.
 *
 *              Plain `unsafe { }` blocks are intentionally NOT flagged
 *              here: the Rust extractor attributes the unsafe blocks
 *              inside standard-library macro expansions (e.g. the
 *              formatting machinery behind eprintln!) to the call site,
 *              which would drown the signal. That vector remains covered
 *              by the compiler-enforced forbid attribute (any unsafe item
 *              is a hard compile error) and the gate-10 textual grep.
 * @kind problem
 * @problem.severity error
 * @id j2r/unsafe-code
 */

import rust

// The CodeQL Rust database contains toolchain/sysroot sources as well as
// the repository itself; scope every finding to repository code (the
// codeql-action runs extraction with the repo root as source root, so all
// repository files have paths under crates/).
predicate repoSrc(AstNode a) {
  a.getLocation().getFile().getRelativePath().regexpMatch("crates/.*")
}

class UnsafeItem extends AstNode {
  UnsafeItem() {
    exists(Function f | this = f and f.isUnsafe() and repoSrc(f))
    or
    exists(Impl i | this = i and i.isUnsafe() and repoSrc(i))
    or
    exists(AsmExpr a | this = a and repoSrc(a))
    or
    exists(UnsafeMeta m | this = m and repoSrc(m))
  }
}

from UnsafeItem u
select u, "J2R forbids unsafe Rust (safe-Rust-only law; CI gate-10)."
