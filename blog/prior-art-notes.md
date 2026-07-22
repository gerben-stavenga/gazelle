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
- **Dijkstra shunting-yard — RESOLVED FOR CITATION.** The EWD/MC
  archive lists MR **35**/61 as "Algol 60 translation" and MR 34/61 as "On the
  design of machine independent programming languages"; but the primary scan's
  title page carries no MR number at all (only "ALGOL Bulletin Supplement
  nr. 10, November 1961"), and one agent argued CWI catalog records attach
  34/61 to the combined report. The paper therefore omits the disputed MR
  number and cites: E.W. Dijkstra, *ALGOL-60 Translation*, Stichting
  Mathematisch Centrum, Rekenafdeling, ALGOL Bulletin Supplement nr. 10, 1961.

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
6. Cite Dijkstra without the disputed MR number.

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
  secondary sources) and [14] Johnson, Yacc CSTR 32, 1975. A subsequent
  primary read of Johnson confirms that the original yacc report describes
  a per-state default action, often a reduction. Joliat's more specific
  error-entry attribution remains search-verified only; read the report
  before restoring that wording. Alternative if Joliat is unobtainable:
  Anderson, Eve &
  Horning, "Efficient LR(1) parsers," Acta Informatica 2:12-39, 1973.
- Bison manual [12]: **VERIFIED 2026-07-22; INTERPRETATION CORRECTED BY
  SUBSEQUENT AUDIT** against 3.8.2 — §5.8.1 LR Table
  Construction (lr.type: lalr/ielr/canonical-lr), §5.8.2 Default Reductions
  (lr.default-reduction: most/consistent/accepting; delayed-detection quote
  confirmed), §5.8.3 LAC (parse.lac), §5.9 GLR (%dprec GLR-only). The manual
  lists `%nonassoc`, default reductions in inconsistent states, and state
  merging as separate causes of delayed or inaccurate error behavior. It
  does not say that defaults overwrite explicit `%nonassoc` error actions;
  §4.3 now uses the manual's three-cause framing.
- Also still queued: extended-production/one-more-dot novelty search; Yang
  formulation check (machine-measured vs grammar-measured).
- The draft no longer claims that no deterministic LR generator offers runtime
  precedence. If that novelty claim is restored, first sweep Menhir, LALRPOP,
  Happy, CUP, lemon, Hyacc, tree-sitter/Lezer (GLR-family, distinguish), ANTLR
  (LL, distinguish), and operator-precedence/Pratt systems (Prolog `op/3`,
  Swift) that offer runtime precedence but are not LR generators.
- **Sweep run 2026-07-22 (single doc-level agent): ORIGINAL CLAIM LATER
  DISPROVED BY AN OMITTED TOOL.** 13
  deterministic LR generators checked (Menhir, LALRPOP, Happy, CUP, Lemon,
  Hyacc, Bison-LALR, Byacc, Racc, PLY, SLY, Rustemo, Jison): all static
  precedence or none. Runtime-decided precedence found only in GLR systems
  (tree-sitter `prec.dynamic` — "applied at runtime… picks the subtree whose
  rule has the highest total dynamic precedence"; Lezer opt-in GLR; Bison
  `%dprec` GLR-only) and non-generator systems (Prolog `op/3`, Pratt/
  precedence climbing, Swift `precedencegroup`, Haskell fixity). A subsequent
  audit found that the sweep omitted parglare, whose deterministic `Parser`
  supports a runtime `dynamic_filter` and whose manual demonstrates
  input-dependent operator precedence. The broad novelty claim has therefore
  been removed.

## Verification sweep 2026-07-22 — five parallel agents (full record)

Five background agents (three haiku, two sonnet), one per open queue item.
Method caveat that applies to agents 4 and 5: the sandbox egress policy
blocked most primary-PDF hosts (arxiv.org, dl.acm.org, mirrors), so those
verdicts are triangulated from tables of contents, verbatim-recurring
abstracts, and secondary literature — the primary texts must still be read
before camera-ready. Agents 1–3 worked against live official documentation
and are correspondingly stronger.

### Agent 1 — runtime-precedence tool survey (haiku, docs-level). ORIGINAL CONCLUSION SUPERSEDED

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
| parglare `Parser` (omitted by agent) | modified LALR/SLR (corrected from LR(1)) | `dynamic_filter` | runtime |

The original agent found parse-time precedence only in GLR systems: tree-sitter
`prec.dynamic` ("applied at runtime instead of at parser generation time …
picks the subtree whose corresponding rule has the highest total dynamic
precedence"), Bison GLR `%dprec` ("first finding one whose rule has the
highest dynamic precedence"), Lezer (opt-in GLR), Happy-GLR. Non-LR
runtime-precedence systems, for the qualifier: Prolog `op/3`
(operator-precedence reader), Pratt/precedence climbing (hand-written),
Swift `precedencegroup`, Haskell fixity. ANTLR is LL. A subsequent
primary-documentation audit found the omitted counterexample: parglare marks
productions `dynamic`, retains the candidate actions, and calls a user
predicate during deterministic LR parsing. Its documented example implements
runtime operator precedence from input-dependent state: the first distinct
operator encountered has the lowest priority and each later one a higher
priority; the callback shifts for a higher-ranked incoming operator and
reduces for a lower or equal one. Gazelle's `prec` policy builds that
shunting-yard comparison into the generated parser and carries pending
precedence on the LR stack, while `conflict` exposes direct token control of
the same binary entry.

API timing caveat checked against `src/runtime.rs` and generated `push`: a
generated `conflict` terminal's `Resolution` is fixed before the reduction
loop and reused for every conflict reached with that lookahead. Reduction
actions may update an external operator stack, but cannot revise that token,
so they cannot switch from reducing a higher pending operator to shifting over
a newly exposed lower one. The low-level parser can reproduce the policy by
calling `maybe_reduce` repeatedly with a freshly chosen resolution; `prec`
provides this repeated comparison in the generated parser and carries the
pending value on its LR stack. Paper framing: `prec` specializes the common
expression-parsing policy; `conflict` remains the general escape hatch. Do not
quantify this as “95%” without supporting usage data.

Corrected action: §5 now presents parglare as the closest precedent and
narrows Gazelle's contribution to its token-carried built-in comparison and
the representation that preserves the choice through completion and
minimization. Appendix A includes parglare and exact source links.

Executable witness 2026-07-23 (user's hunch: parglare fails the subtle
precedence case because of merge conflicts — CONFIRMED, sharpened; script
blog/parglare_witness.py, parglare 0.21.1 in-sandbox):
- Grammar S: 'a' M | 'b' M Z; M: E {dynamic} | E Z {dynamic}; E: 'x';
  terminal Z {dynamic}. Canonical: a-context shifts z unconditionally;
  b-context has genuine S/R on z. parglare merges the same-core states
  (kernel lookaheads {STOP,Z} = union).
- Default (prefer_shifts=True): the genuine question is silently resolved
  to shift AT CONSTRUCTION despite dynamic markers — reduce M->E on z
  absent from the table entirely; filtering the remaining candidate cannot
  recover valid 'b x z'.
- prefer_shifts=False: merged cell defers in BOTH contexts; 'a x z' and
  'b x z' reach the same state with the same remaining input but need
  opposite actions — a local SHIFT policy fails 'b x z', while a local
  REDUCE policy fails 'a x z' (both clean SyntaxErrors on valid input). No
  filter of (state, action, subresults, remaining input) can be correct.
- IMPORTANT SCOPE CORRECTION: parglare exposes the full input and arbitrary
  `context.extra` state. A history-aware filter that rereads the consumed
  prefix parses all four witness inputs; a shadow canonical parser could do
  so in general. Thus the witness separates TABLE-LOCAL resolution, not the
  expressiveness of unrestricted Python callbacks. Morally, allowing the
  callback to reconstruct the erased parser context is the oracle escape the
  paper excludes explicitly.
- Documentation/default defect independently reproduced: the manual shows
  `Parser(grammar, dynamic_filter=...)`, but the corresponding project test
  adds `prefer_shifts=False`. Under the documented/default call the two sample
  expressions both evaluate to 17 rather than the documented 14 and 21,
  because construction has already discarded the reductions.
- parglare GLRParser parses all four (fork-and-die); the failure is
  specific to deterministic deferral over merged tables.
- Side findings: rejecting the sole candidate action crashes parglare
  with unhandled IndexError; filter receives an all-None initialization
  probe call; state-2 item lookaheads suggest FOLLOW-flavored sets.
- Paper: Appendix A.4 defines the table-local boundary, demonstrates the
  failure within it, and shows the history-aware escape. This upgrades the §5
  architectural observation to a demonstrated table-faithfulness failure for
  THIS shape (one tool, one version, one grammar — scoped accordingly).

Independent re-verification 2026-07-23 (primary docs via raw.githubusercontent):
parglare's disambiguation page's worked example instantiates the
deterministic class — `Parser(grammar, dynamic_filter=custom_disambiguation_filter)` —
with filter signature (context, from_state, to_state, action, production,
subresults) deciding SHIFT/REDUCE. Its precedence example records operators
in order of first appearance and uses that evolving order in the filter.
The nearby sentence saying "these markers have sense only for GLR parsing"
refers to `nops` and `nopse`, not to `dynamic`, so there is no documentation
contradiction. parser.md says that modified LALR tables are used by default;
the `tables` parameter alternatively offers SLR, but no canonical LR(1).
Appendix A records that corrected construction. Parglare therefore defers
over merged tables with no documented canonical-faithfulness analysis,
which is the obligation Gazelle's construction addresses.

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

### Agent 3 — Bison manual (haiku, against gnu.org 3.8.2). EVIDENCE VERIFIED; ORIGINAL INTERPRETATION CORRECTED

§5.8.1 "LR Table Construction" (`lr.type`: lalr/ielr/canonical-lr);
§5.8.2 "Default Reductions" (`lr.default-reduction`: most/consistent/
accepting; delayed-detection passage quoted: "the parser sometimes fails
to detect the syntax error until it reaches a later state"); §5.8.3 LAC
(`parse.lac`: none/full; "solves these problems for canonical LR, IELR,
and LALR without sacrificing %nonassoc, default reductions, or state
merging"); §5.9 "Generalized LR (GLR) Parsing" (`%dprec` GLR-only).

Subsequent primary-source audit correction: the quoted evidence does not
establish `%nonassoc` masking. The default-reduction section distinguishes
an absent action from an explicit `%nonassoc` error action; a default makes
the former impossible but does not overwrite the latter. The LAC section
names `%nonassoc`, inconsistent-state defaults, and state merging as distinct
culprits.

Action taken: [12] cites version + section numbers; §4.3 now states the three
causes separately and describes LAC as addressing them collectively.

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
added; §4.3 and §7 sentences reworded. A subsequent primary read verified
the yacc report's per-state default reduction. Joliat's specific
error-entry attribution remains unverified and has been removed from the
paper; AEH 1973 remains the fallback if that report cannot be obtained.

### Agent 5 — Yang arXiv:2110.00776 (sonnet, PDFs blocked). RECONSTRUCTED, THEN PRIMARY-VERIFIED

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
Parsing," ICAS 2018 (moderate confidence).

The later primary read confirmed the graph-to-grammar reduction, the
4n^2−2n+3 state count, pairwise conflict and successor constraints, and the
absence of completion language. It also caught a terminology error outside
the agent's reconstruction: a nonoptimal quotient is potentially *finer*,
not coarser, than the quotient produced by a better merge scheme.

Action taken: §4.5 rewritten to state Yang's result as reconstructed, with
the completion equivalence explicitly marked as our observation; related-
work sentence aligned; disclaimer sharpened (nothing claims to reach
Yang's optimum).

### Sweep cost and residue

~334k subagent tokens, five agents, ~7 minutes wall-clock. Subsequent audit
completed the primary reads of Yang and yacc CSTR 32. Remaining before
submission: a primary read of Joliat; C++ row
into the automated Bison regression; the old 42/100 sweep's unfinished
votes (Pager, Honalee, IELR — none currently load-bearing).

## Still outstanding (workflow died at 42/100 agents)

Verification votes for Pager/Honalee/IELR/langcc/Hyacc/Yang/Dijkstra; final
synthesis. Resumable with all completed agents cached — see
`~/.claude/projects/.../memory/prior-art-sweep.md` for the resume handle.
