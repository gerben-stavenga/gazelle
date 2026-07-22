# Prior-art sweep for minimal-lr1-tables.md

Deep-research run, 2026-07-17. 42/100 agents completed before hitting the session
limit; search and deep-read stages are essentially complete, adversarial
verification finished only for Kannapinn / Heilbrunner / Scott&Johnstone (every
vote 3-0 confirmed against primary sources). Items marked **[unverified]** were
extracted from primary sources by one agent but not adversarially checked.

Claims under test:

- **Claim 1** — reduce actions become ordinary labeled transitions to per-rule
  reduce states, so the whole parse table is one DFA amenable to generic subset
  construction and minimization.
- **Claim 2** — resolve conflicts on the full canonical LR(1) automaton first,
  then minimize with standard DFA techniques, landing on IELR-sized tables for
  non-LR(1) grammars.

## Bottom line

- **Claim 2 is partially anticipated.** The paper must cite Sönke Kannapinn,
  *Eine Rekonstruktion der LR-Theorie zur Elimination von Redundanz*, TU Berlin
  dissertation, 2001
  (https://webdoc.sub.gwdg.de/ebook/ah/2003/tu-berlin/kannapinn_soenke.pdf).
- **Claim 1 survives**, but Kannapinn's Mealy remark and RIGLR must be cited and
  distinguished.
- Everything else surveyed (Pager, Honalee, IELR, langcc, Hyacc, Heilbrunner)
  is merge/split-during-construction or a different encoding — supporting the
  novelty framing, subject to the unverified items below.

## Kannapinn 2001 (verified 3-0, against the primary PDF)

- Constructs minimal-size general LR(k) parsers by **post-hoc minimization of
  the completed canonical LR(k) machine** — Hopcroft (1971)/Gries (1973)
  partition refinement, seeded not with the accept/non-accept bipartition but
  with a partition derived from right-context/reduce information treated as
  **Moore-style state output** (diss. ch. 4, p. 54). English abstract:
  "We demonstrate how to construct general LR(k) parsers of minimal size by
  applying minimization techniques from automata theory..."
- **Explicitly rejected bare-DFA minimization** of the canonical machine because
  reduce/lookahead information is absent from the bare transition structure
  (pp. 39-40: "…die Idee der DEA-Minimierung der deterministischen kanonischen
  LR(k)-Maschine … verworfen werden muß"). This is precisely the obstacle the
  gazelle encoding removes — quote it, then present Claim 1 as the move that
  makes the rejected idea sound.
- Notes the equivalent **Mealy variant** (right-context attached to transitions)
  and proves both yield structurally identical minimal machines. He does NOT
  encode reduces as ordinary labeled transitions to per-rule sink states run
  through subset construction — closest prior art to Claim 1, but not it.
- No conflict resolution before minimization (assumes conflict-free LR(k)), no
  spurious-reduce alignment, no non-LR(1)/prec grammars, no empirical IELR
  comparison. That list is gazelle's residual contribution for Claim 2.

## Scott & Johnstone, RIGLR / RI parsing (verified 3-0)

- *Generalized Bottom Up Parsers With Reduced Stack Activity*, Computer Journal
  48(5):565-587, 2005; and the SCP 2007 paper (S0167642307000731).
- Reductions ARE incorporated into the automaton — but as edges connecting a
  reduction state directly to its goto state, to **suppress stack activity** for
  non-embedded recursion in GLR-family parsing. Purpose is runtime performance,
  not table minimization; encoding is not follow-token-labeled edges into
  per-rule reduce states. Cite in §8 and distinguish on both axes.

## Heilbrunner 1981 (verified 3-0)

- *A Parsing Automata Approach to LR Theory*, TCS 15:117-157. Item grammars +
  parsing automata as a framework general enough to subsume DeRemer and Pager.
  Genuine automata-theoretic treatment of LR parsing, but no
  reduce-as-transitions and no whole-table-as-one-DFA. Worth a citation as the
  automata-theoretic tradition Claim 1 sits in.

## Merge-during-construction family (all [unverified] — single-agent reads of primary sources)

- **Pager 1977** (Acta Informatica 7:249-268): combines states *as they are
  generated* under weak/strong compatibility; no resolve-then-minimize.
- **Honalee (Tribble)**: "merge as you go"; explicitly considered
  generate-all-canonical-states-then-merge and **rejected it as impractical** —
  nice foil: gazelle does the "naive" thing and shows it works.
- **IELR(1) (Denny & Malloy)**: LALR(1) + inadequacy-driven state splitting;
  precedence/associativity resolution is the **final phase (Phase 5)** — the
  opposite ordering of gazelle. Their observation that Pager-style merging
  loses canonical-LR(1) power exactly when the grammar is non-LR(1) with a
  conflict-resolution spec is the failure mode gazelle's ordering avoids —
  strongest related-work hook for Claim 2.
- **langcc (Zimmerman)** [second sweep 2026-07-22, search-level]: CONFIRMED —
  CPS grammar transformation, optimized NFA construction, construction-time
  k-follow-set partitioning (size attacked during construction, not post
  hoc). NOT CONFIRMED (do not reuse without reading the full paper):
  "reduces are vertex accept-actions", "backward conflict propagation on the
  LR(0) NFA", "rejects Pager-style processing on efficiency grounds".
  resolve-then-minimize.md §7 now cites langcc [13] using only the confirmed
  characterization.
- **Hyacc** [second sweep 2026-07-22, search-level]: CONFIRMED — Pager's PGM
  with weak-compatibility merging during construction; UPE + UPE-Ext as the
  post-hoc steps (Pager's unit-production elimination + extension). NOT
  CONFIRMED: the "duplicate-action-row hash merge" phrasing — drop it.

## Citation checks

- **Wuu Yang, arXiv:2110.00776, "Minimizing LR(1) State Machines is NP-Hard"**
  [unverified, but two independent agents agree]: author's name is "Wuu Yang"
  (not "W. Yang"), submitted 2021-10-02. NP-hardness via node-coloring → CFG →
  LR(1) machine. Note this concerns *optimal* merging; Kannapinn's (and
  gazelle's) minimization is over a fixed equivalence, so no contradiction —
  but the paper should phrase its "minimal" claim carefully against this.
- **Dijkstra shunting-yard — UNRESOLVED, check before submission.** The EWD/MC
  archive lists MR **35**/61 as "Algol 60 translation" and MR 34/61 as "On the
  design of machine independent programming languages"; but the primary scan's
  title page carries no MR number at all (only "ALGOL Bulletin Supplement
  nr. 10, November 1961"), and one agent argued CWI catalog records attach
  34/61 to the combined report. Safest citation: E.W. Dijkstra, *ALGOL-60
  Translation*, Stichting Mathematisch Centrum, Rekenafdeling, ALGOL Bulletin
  Supplement nr. 10, 1961.

## Suggested paper edits (agreed in review, not yet applied)

1. Cite Kannapinn in §2/§8; frame Claim 1 as removing the obstacle he named,
   Claim 2's novelty as resolve-*before*-minimize + alignment + IELR-exactness.
2. Cite and distinguish RIGLR/RI in §8 (purpose and encoding differ).
3. Soften "lands exactly on IELR" to structural correctness + empirical size
   equality (C++ 601, C11 470/506).
4. Add the §6 guard paragraph: alignment must not fill a virtual prec reduce
   edge into a state that transitions on the real twin (fix/prec-alignment-guard,
   commit 8034a5a; C11 costs 36 states: 470→506).
5. Fix the 572/571 state-count inconsistency.
6. Resolve the Dijkstra MR number.

## Added 2026-07-21: default-reduction framing (resolve-then-minimize.md §4.3)

- New claim to verify: completion = classical default reductions under a new
  selection policy; classical per-state defaults are a special case of the
  insertion conditions (error entry only + extends an existing reduction's
  lookahead set).
- **[unverified]** Aho & Johnson, "LR parsing," ACM Computing Surveys 6(2),
  1974 — cited [11] for default reductions/table compaction; confirm it
  actually discusses default actions on error entries and delayed error
  detection.
- Bison manual `%define lr.default-reduction` cited [12] — confirm exact
  section name and semantics (most/consistent/accepting values); also check
  bison's documented %nonassoc caveat matches our §4.3 non-fillable note.
- Also still queued: extended-production/one-more-dot novelty search; Yang
  formulation check (machine-measured vs grammar-measured).
- The draft no longer claims that no deterministic LR generator offers runtime
  precedence. If that novelty claim is restored, first sweep Menhir, LALRPOP,
  Happy, CUP, lemon, Hyacc, tree-sitter/Lezer (GLR-family, distinguish), ANTLR
  (LL, distinguish), and operator-precedence/Pratt systems (Prolog `op/3`,
  Swift) that offer runtime precedence but are not LR generators.
- **Sweep run 2026-07-22 (single doc-level agent): claim CONFIRMED.** 13+
  deterministic LR generators checked (Menhir, LALRPOP, Happy, CUP, Lemon,
  Hyacc, Bison-LALR, Byacc, Racc, PLY, SLY, Rustemo, Jison): all static
  precedence or none. Runtime-decided precedence found only in GLR systems
  (tree-sitter `prec.dynamic` — "applied at runtime… picks the subtree whose
  rule has the highest total dynamic precedence"; Lezer opt-in GLR; Bison
  `%dprec` GLR-only) and non-generator systems (Prolog `op/3`, Pratt/
  precedence climbing, Swift `precedencegroup`, Haskell fixity). Safe to
  restore the §5 claim with its "to our knowledge" hedge and the existing
  GLR/`%dprec` distinction.

## Still outstanding (workflow died at 42/100 agents)

Verification votes for Pager/Honalee/IELR/langcc/Hyacc/Yang/Dijkstra; final
synthesis. Resumable with all completed agents cached — see
`~/.claude/projects/.../memory/prior-art-sweep.md` for the resume handle.
