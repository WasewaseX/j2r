# scripts/evidence — loose .java policy (2026-11-19)

The scanaislop round-8 wave flags loose probe sources under
`scripts/evidence/` with the corpus complexity thresholds (function above
88 lines, file above 441). The probe sources are byte-frozen evidence, so
the fix follows the round-4 end-state doctrine (reshape was never an
option for frozen bytes): freeze the flagged sources into this
`probe_sources_frozen_2026-11-19.tar.gz` archive and drop the loose
copies from the scan surface.

Contents (extract with `tar -xzf probe_sources_frozen_2026-11-19.tar.gz`
from `scripts/evidence/`):

- `char_array_rep/CharArrayRepProbe.java` — byte-identical to the live
  fixture `crates/j2r-cli/tests/suite/CharArrayRepProbe.java` (the
  GREEN/RED proof records reference the suite copy for reproduction).
- `g3_3/TreeMapLhmPrimProbe.java` — byte-identical to the live fixture
  `crates/j2r-cli/tests/suite/TreeMapLhmPrimProbe.java`.
- `treemap_views/TreeMapViewsProbe.java` — byte-identical to the live
  fixture `crates/j2r-cli/tests/suite/TreeMapViewsProbe.java`.
- `treemap/TreeMapProbe.java` — historical variant, DIFFERS from the live
  suite fixture by 18 diff lines; the only unique bytes in the freeze.

The proof records (GREEN_PROOF/RED_PROOF .txt files beside each probe)
stay loose: they document the fix history and reference the suite
fixtures, not the frozen bytes.
