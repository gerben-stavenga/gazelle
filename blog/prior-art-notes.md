# Prior-art sweeps for minimal-lr1-tables.md and resolve-then-minimize.md

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

- **Wuu Yang, arXiv:2110.00776** [third check 2026-07-22, search-level
  reconstruction; PDF unreachable in-session — READ PRIMARY BEFORE
  CAMERA-READY]: abstract quote confirmed verbatim (node-coloring reduced
  indirectly: graph → CFG → canonical LR(1) machine, incremental
  construction from a two-node template). Reconstruction: machine has
  4n^2-2n+3 states (poly in graph); graph k-colorable iff n-k similar-state
  pairs mergeable; constraints = no reduce/reduce conflict + successor
  consistency. IMPORTANT: hardness is merge-selection on a GIVEN machine;
  the old "undefined entries/completions/non-transitive compatibility"
  mechanism attribution was OUR framing, not Yang's — resolve-then-minimize
  §4.5 rewritten accordingly (completion view retained but explicitly
  marked ours). Possible earlier version: "Extended LALR(1) Parsing,"
  ICAS 2018 (moderate confidence). Verify against PDF: exact theorem
  statement; whether any don't-care/completion language appears; the
  state-count formula.
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
- Aho & Johnson 1974: **CHECKED 2026-07-22, verdict likely-wrong for the
  default-reduction claim** (its optimization section is state
  merging/subsuming/unit-production elimination, not row defaults; primary
  PDF unreachable in-session, verdict from ToC + ~10 corroborating
  searches). REPLACED in resolve-then-minimize.md by [11] Joliat 1973
  (CSRG-28, "first to suggest factoring out the error entries" per
  secondary sources) and [14] Johnson, Yacc CSTR 32, 1975 (earliest
  implementation, per-state yydefact defaults). Both replacements are
  search-verified only — read Joliat's TR and the yacc report before
  camera-ready. Alternative if Joliat is unobtainable: Anderson, Eve &
  Horning, "Efficient LR(1) parsers," Acta Informatica 2:12-39, 1973.
- Bison manual [12]: **VERIFIED 2026-07-22** against 3.8.2 — §5.8.1 LR Table
  Construction (lr.type: lalr/ielr/canonical-lr), §5.8.2 Default Reductions
  (lr.default-reduction: most/consistent/accepting; delayed-detection quote
  confirmed), §5.8.3 LAC (parse.lac), §5.9 GLR (%dprec GLR-only). Manual
  documents the %nonassoc-masking caveat explicitly — now cited in §4.3.
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

## Verification sweep 2026-07-22 — five parallel agents (full record)

Five background agents (three haiku, two sonnet), one per open queue item.
Method caveat that applies to agents 4 and 5: the sandbox egress policy
blocked most primary-PDF hosts (arxiv.org, dl.acm.org, mirrors), so those
verdicts are triangulated from tables of contents, verbatim-recurring
abstracts, and secondary literature — the primary texts must still be read
before camera-ready. Agents 1–3 worked against live official documentation
and are correspondingly stronger.

### Agent 1 — runtime-precedence tool survey (haiku, docs-level). CONFIRMED

Question: does any deterministic LR parser generator keep both actions of a
conflicted cell in its tables and decide at parse time from token data?

| tool | construction (corrected where noted) | precedence | verdict |
|---|---|---|---|
| GNU Bison (LR modes) | LALR/IELR/canonical | %left/%right/%nonassoc/%prec | static |
| Berkeley Yacc | LALR | yacc declarations | static |
| Menhir | LR(1), Pager (agent said LALR — corrected) | %left/%right/%nonassoc | static |
| Happy | LALR | %left/%right/%nonassoc | static |
| CUP | LALR | declarations + contextual %prec | static |
| Lemon | LALR | %left/%right/%nonassoc | static |
| Hyacc | LR(1) PGM (agent said "LALR variant" — corrected) | yacc declarations | static |
| LALRPOP | LR(1) lane table | none | n/a |
| PLY | LALR | precedence tuple | static |
| SLY | LALR | precedence attribute | static |
| Racc | LALR | precedence table | static |
| Rustemo (LR mode) | LR | static declarations | static |
| Jison (LR modes) | SLR/LALR/LR | %left/%right/%nonassoc | static |

Parse-time precedence found only in GLR systems: tree-sitter
`prec.dynamic` ("applied at runtime instead of at parser generation time …
picks the subtree whose corresponding rule has the highest total dynamic
precedence"), Bison GLR `%dprec` ("first finding one whose rule has the
highest dynamic precedence"), Lezer (opt-in GLR), Happy-GLR. Non-LR
runtime-precedence systems, for the qualifier: Prolog `op/3`
(operator-precedence reader), Pratt/precedence climbing (hand-written),
Swift `precedencegroup`, Haskell fixity. ANTLR is LL. Sources: official
manuals, URLs in resolve-then-minimize.md Appendix A.

Action taken: §5 novelty claim restored with survey basis; Appendix A added
to resolve-then-minimize.md with tables, quotes, and scope statement.

### Agent 2 — langcc / Hyacc characterization (haiku, search-level). PARTIAL

langcc CONFIRMED: CPS grammar transformation ("a novel transformation for
LR grammars we call 'continuation-passing style'"), optimized NFA
construction ("drastically reduces the number of states required"),
construction-time k-follow-set partitioning, XLR bounded nondeterminism.
NOT FOUND (do not reuse): "vertex accept-actions", "backward conflict
propagation on the LR(0) NFA", "rejects Pager-style processing on
efficiency grounds". Hyacc CONFIRMED: Pager's PGM with weak-compatibility
merging during construction; UPE + UPE-Ext as post-hoc unit-production
elimination. NOT FOUND: "duplicate-action-row hash merge".

Action taken: langcc restored to related work as [13] using only the
confirmed characterization; unverified phrases marked do-not-reuse above.

### Agent 3 — Bison manual (haiku, against gnu.org 3.8.2). FULLY VERIFIED

§5.8.1 "LR Table Construction" (`lr.type`: lalr/ielr/canonical-lr);
§5.8.2 "Default Reductions" (`lr.default-reduction`: most/consistent/
accepting; delayed-detection passage quoted: "the parser sometimes fails
to detect the syntax error until it reaches a later state"); §5.8.3 LAC
(`parse.lac`: none/full; "solves these problems for canonical LR, IELR,
and LALR without sacrificing %nonassoc, default reductions, or state
merging"); §5.9 "Generalized LR (GLR) Parsing" (`%dprec` GLR-only). The
manual explicitly documents the %nonassoc-masking caveat.

Action taken: [12] now cites version + section numbers; §4.3's %nonassoc
caveat upgraded to "documented, with LAC as Bison's repair".

### Agent 4 — Aho & Johnson 1974 (sonnet, PDFs blocked). LIKELY WRONG REF

Bibliography exactly correct (ACM Comput. Surv. 6(2):99–124, June 1974,
doi:10.1145/356628.356629). But the reconstructed table of contents shows
its "Optimization of LR Parsers" section covers merging identical states,
subsuming states, and unit-production elimination — state-count reduction,
not row-level default reductions. ~10 searches found no link between this
survey and default reductions; Xin Chen's dissertation (fetched, searched)
discusses default reductions (yacc `yydefact`) citing a different lineage.
Recommended replacements, ranked: Anderson–Eve–Horning 1973 (Acta Inf.
2:12–39); Joliat 1973 (CSRG-28, Toronto — "probably the first to suggest
factoring out the error entries"); Johnson, Yacc CSTR 32, 1975 (earliest
implementation); Dragon Book §4.7 (modern treatment).

Action taken: [11] replaced by Joliat 1973; [14] Johnson Yacc CSTR 32
added; §4.3 and §7 sentences reworded. Joliat and the yacc TR are
search-verified only — read before camera-ready; AEH 1973 is the fallback.

### Agent 5 — Yang arXiv:2110.00776 (sonnet, PDFs blocked). RECONSTRUCTED

Abstract confirmed verbatim (recurred identically across queries):
node-coloring reduced *indirectly* — graph → CFG → canonical LR(1)
machine — with incremental construction from a two-node template grammar.
High-confidence paraphrase: constructed machine has 4n^2−2n+3 states
(polynomial in the graph); graph k-colorable iff n−k "similar"
(same-core) state pairs mergeable; merge constraints = no reduce/reduce
conflict + successor consistency (merging two states requires their
successors merged). KEY FINDING: the hardness is merge-selection on a
GIVEN, deterministically built machine; no "don't-care/completion/
non-transitive compatibility" language could be found in Yang's own text —
that framing was ours. Possible earlier version: "Extended LALR(1)
Parsing," ICAS 2018 (moderate confidence). Before camera-ready: pull the
PDF and confirm the numbered theorem, the state-count formula, and the
absence of completion language.

Action taken: §4.5 rewritten to state Yang's result as reconstructed, with
the completion equivalence explicitly marked as our observation; related-
work sentence aligned; disclaimer sharpened (nothing claims to reach
Yang's optimum).

### Sweep cost and residue

~334k subagent tokens, five agents, ~7 minutes wall-clock. Remaining
before submission: primary reads of Yang, Joliat, yacc CSTR 32; C++ row
into the automated Bison regression; the old 42/100 sweep's unfinished
votes (Pager, Honalee, IELR — none currently load-bearing).

## Still outstanding (workflow died at 42/100 agents)

Verification votes for Pager/Honalee/IELR/langcc/Hyacc/Yang/Dijkstra; final
synthesis. Resumable with all completed agents cached — see
`~/.claude/projects/.../memory/prior-art-sweep.md` for the resume handle.
