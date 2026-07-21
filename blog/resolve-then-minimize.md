# Resolve, Then Minimize: LR Parse Tables as Automata

## Abstract

Canonical LR(1) construction preserves the context needed for correct conflict
resolution, but its tables are often too large to ship. Practical generators
therefore merge states early and then prevent or repair changes caused by the
merge. This paper explores the opposite order: construct the canonical
automaton, resolve conflicts there, and minimize only the resulting behavior.

The key representation is to encode a reduction of production *r* on
lookahead *a* as an ordinary transition labeled *a* into an accepting state
distinguished by *r*. Shifts, gotos, and reductions then inhabit one labeled
transition system. This exposes the central observation: subset construction
removes nondeterminism in *where the machine goes*, but a resulting subset may
still contain incompatible answers about *what the parser should do*. An item
state together with a reduce state is a shift/reduce conflict; two reduce
states are a reduce/reduce conflict. Conflicts are therefore precisely the
output nondeterminism left after determinization.

Canonical LR(1) construction becomes subset construction on the item NFA;
conflict resolution becomes classification of the ambiguous accepting states;
and table compression becomes ordinary partition refinement after a small
completion pass fills selected error entries with reductions. The completion
may delay the detection of invalid input, but it preserves every shift and
reduction performed on valid input.

Gazelle implements this pipeline using the same generic automaton module as
its lexer generator. On five grammars, its canonical item-set counts and
conflict counts agree with GNU Bison's canonical-LR mode. After completion and
minimization, its state counts agree with Bison's IELR mode: a 292-line C++
grammar falls from 5,350 canonical states to 601, while LALR has 571 states and
would change resolved behavior. Equality of state counts is evidence about
compactness, not a proof that the two constructions produce isomorphic
machines. The result is instead a simpler route to a canonical-LR-equivalent
parser whose size is, on the evaluated grammars, the same as IELR.

## 1. Introduction

Canonical LR(1) gives a parser generator an attractive semantic baseline. Its
states retain the complete one-token right context derived from the grammar,
so a conflict is inherent in the grammar rather than introduced by table
construction. If a generator resolves conflicts in this automaton, the
resolution is applied in precisely the contexts in which each action arose.

The difficulty is size. The canonical automaton for the C++ grammar used in
this paper contains 5,350 item states. The corresponding LALR automaton has
571. This gap explains the usual construction order: build or approximate the
small machine first, then perform conflict analysis on it. Unfortunately,
merging LR(1) contexts can create new conflicts, and it can also change how a
real conflict is resolved. Pager's method restricts construction-time merges
to compatible states [3]. IELR begins from LALR and splits states whose merges
would alter canonical-LR behavior [4]. These algorithms differ in detail, but
both must reason about the interaction between merging and resolution.

Gazelle reverses the dependency:

```text
grammar
   -> item NFA
   -> subset construction
   -> canonical conflict resolution
   -> conservative completion
   -> partition refinement
   -> parse table
```

This order makes the semantic reference machine concrete. Resolution happens
before information is discarded. Later stages may merge states only when the
resolved transition system cannot distinguish them.

That description conceals a representational obstacle. A conventional LR
table contains transitions for shifts and gotos, but reductions are actions
attached to table cells or states. Generic DFA minimization cannot preserve
information absent from the transition relation. Gazelle removes the obstacle
by representing `reduce r on a` as a transition on `a` into a per-production
accepting state. A parser action is determined by the kind of state reached:
reaching an item state shifts, while reaching reduce state *r* reduces by
production *r*. If determinization reaches both kinds at once, it has made the
transition deterministic without making the parser's answer deterministic:
that residual choice is the conflict. Reduce states are distinguished in the
initial partition during minimization.

This paper makes four contributions:

1. It gives an item-NFA encoding in which the complete LR table, including
   reductions, is one labeled transition system.
2. It identifies LR conflicts with output nondeterminism remaining in subset
   states after transition nondeterminism has been eliminated.
3. It gives a conservative completion rule that exposes common LALR-style
   merges to generic partition refinement while preserving valid-input parser
   behavior.
4. It evaluates the construction against Bison's canonical-LR and IELR modes
   and describes runtime precedence as an application of the representation.

The claim is deliberately narrower than global minimality. Finding the
smallest conflict-free merge of canonical LR(1) states is NP-hard in general
[5]. Gazelle chooses one deterministic completion of the transition system and
computes the unique coarsest behavioral quotient of that completed system.
Another completion could, in principle, enable a smaller sound quotient.

## 2. Background and problem statement

Let a grammar production be

```text
r: A -> X1 ... Xn
```

and let an LR(1) item be `[A -> alpha . beta, a]`, where `a` is the terminal
that may follow this occurrence of `A`. Closure introduces productions for a
nonterminal after the dot, with lookaheads selected from `FIRST(beta a)`.
Goto advances the dot over one grammar symbol. The canonical LR(1) automaton
is the collection of item sets reachable under closure and goto [1, 6].

At runtime an LR parser keeps a stack of automaton states and holds one input
token in hand. A shift follows a terminal transition and pushes its target. A
reduction by `A -> beta` removes `|beta|` states, exposes a predecessor state,
and follows its transition on `A`. The lookahead token is not consumed by a
reduction; several reductions may occur before it is finally shifted.

In a conventional presentation, the automaton transition relation supplies
shift and goto entries while a separate action table supplies reductions. If
state *q* contains completed item `[A -> beta ., a]`, cell `(q, a)` receives
`reduce A -> beta`. A shift/reduce conflict occurs if a shift is also present
in that cell, and a reduce/reduce conflict occurs if two completed items
install different reductions there.

### 2.1 Why merge-first construction is difficult

LALR merges canonical states with the same LR(0) core: the same `(production,
dot)` pairs after lookaheads are erased. Consider the standard LR(1)-but-not-
LALR grammar:

```text
s = A x A | B x B | A y B | B y A;
x = T;
y = T;
```

After `A T`, one canonical state reduces `T` to `x` on `A` and to `y` on `B`.
After `B T`, another state has the reverse mapping. Their LR(0) cores are the
same, but merging them unions the lookaheads and creates reduce/reduce
conflicts on both terminals. The grammar is LR(1); the conflicts are artifacts
of the approximation.

The same mechanism is more subtle when the canonical automaton already has
conflicts. Suppose an ambiguous grammar uses the conventional policy “shift
wins.” A merge can import a reduce action into a context that previously had
only a shift. Resolving the merged cell still chooses shift, but it now chooses
shift in both contributing contexts. A declaration or rule ordering can
similarly affect contexts from which it did not originate. Thus “the conflicts
were resolved” is insufficient: the merged parser may differ from the
canonical parser with the same resolution policy.

IELR addresses this problem by tracking which lookahead contributions make a
LALR state inadequate and splitting it until resolved behavior agrees with the
canonical reference [4]. Gazelle instead constructs that reference, resolves
it directly, and asks a generic minimizer which resolved states remain
distinguishable.

### 2.2 Observable behavior

Table compression requires a precise boundary. For this paper, two parsers
have the same valid-input behavior when, for every token string accepted by
the canonical resolved parser, they perform the same ordered sequence of
shifts and reductions and therefore construct the same parse tree. They must
also reject every string rejected by that parser.

They need not report an invalid string at the same token or after the same
number of reductions. This distinction permits **spurious reductions**:
reductions taken only on paths that cannot lead to acceptance. A spurious
reduction can delay an error, but it cannot turn an invalid string into a
valid one when completion and minimization satisfy the conditions in §4.

This equivalence is appropriate for recognition and valid-input semantics. It
does not preserve all observations a caller might make during a failed parse.
In particular, semantic actions with external side effects may run on a doomed
path before the error is reported. Applications requiring transactional
behavior on invalid input must buffer or roll back such effects independently.

## 3. Encoding the complete table as an automaton

The construction begins with an NFA whose ordinary states are LR(1) items.
For each item `[A -> alpha . X beta, a]`, add a transition labeled `X` to
`[A -> alpha X . beta, a]`. If `X` is a nonterminal, add epsilon transitions
to `[X -> . gamma, b]` for its productions and every
`b in FIRST(beta a)`. This is the familiar item-NFA formulation of canonical
LR construction [6, 7].

Gazelle adds one accepting state `R_r` for every production *r*. For every
completed item `[A -> beta ., a]`, it adds

```text
[A -> beta ., a] --a--> R_r .
```

The accepting state is distinguished by production *r*: reaching it recognizes
a completed right-hand side and emits the action “reduce by *r*.” This is the
same role that a token-distinguished accepting state plays in a lexer NFA.
Only `R_r` for the augmented start production denotes acceptance of the whole
parse; every other accepting state returns control to the pushdown parser,
which performs the reduction and continues.

An equivalent reading is to extend every production by its lookahead and let
the dot advance one more position:

```text
A -> beta . a  --a-->  A -> beta a .
```

All NFA edges are therefore either epsilon closure edges or ordinary dot
advances. The extra completed position is shared by all lookaheads of a
production because the runtime needs only the production identity after the
edge has been followed.

Subset construction now produces states containing both item nodes and reduce
nodes. Its transition relation has a direct runtime interpretation:

- an edge on a nonterminal whose target contains items is a goto;
- an edge on a terminal whose target contains items is a shift;
- an edge on a terminal whose target is `R_r` is reduction *r*.

For a conflict-free LR(1) grammar, every reachable target used by the parser is
either an item set or one reduce node. If a subset contains incompatible kinds
of answers, determinization has succeeded as an automaton construction but has
not settled the parser action. That case is §4.1. Otherwise the parser loop is
small:

```text
push(token a):
  loop:
    target = delta(top(stack), a)
    if target is item state q:
      push q; consume a; return
    if target is reduce state R_r for A -> beta:
      pop |beta| states
      push delta(top(stack), A)
      if r is the augmented start rule: accept
    otherwise:
      report an error
```

The stack still matters—the parser is not a finite-state recognizer—but its
table is a finite labeled transition system. The distinction matters because
generic subset construction and partition refinement operate on the table,
not on the pushdown control surrounding it.

### 3.1 Correspondence with canonical LR(1)

Ignoring reduce nodes, the epsilon closure and symbol transitions are exactly
those of the standard LR(1) item NFA. Subset construction therefore yields the
same reachable item sets as canonical closure/goto construction. A completed
item contributes an outgoing transition on exactly its LR(1) lookahead, so the
transition into `R_r` exists exactly where the conventional action table would
contain `reduce r`.

This gives a direct correspondence rather than a new parsing method:

- canonical item sets correspond to the item projection of subset states;
- shift and goto entries correspond to item-targeting edges;
- reduce entries correspond to edges into rule-distinguished reduce states.

The representation changes where actions live, not which actions the
canonical construction computes.

**Proposition 1 (canonical correspondence).** Projecting reduce nodes out of
each reachable subset state yields the canonical LR(1) item set reached by the
same viable prefix. For every terminal and nonterminal, the encoded machine
contains the corresponding shift, goto, or reduce edge if and only if the
conventional canonical table contains that action.

The argument follows directly by induction over subset construction: epsilon
closure is LR(1) closure, symbol advance is goto, and the only additional edge
maps each completed item to the reduction already prescribed by its
lookahead.

## 4. Resolve first, complete, then minimize

### 4.1 The nondeterminism that remains

Subset construction is deterministic: it produces at most one outgoing edge
per symbol. But this removes only **transition nondeterminism**. It says where
the machine goes; it does not guarantee that every NFA state collected at the
destination assigns the same meaning to arriving there.

This distinction is familiar in lexer generation. If a determinized regex
state contains accepting NFA states for both `IDENT` and `KEYWORD`, the DFA has
one transition path but two possible token outputs. The lexer generator still
needs a priority rule. Its residual problem is not graph nondeterminism but
**output nondeterminism**.

The reduce-state encoding makes LR conflicts the same phenomenon. A conflict
cannot appear as two competing DFA edges; it appears in their union target.

If an item advances on terminal `a` while a completed item reduces on `a`, the
target of the single `a` edge contains both advanced items and a reduce node.
This is a **hybrid state**: the deterministic destination says both “shift” and
“reduce *r*.” If two completed items reduce on `a`, the target contains two
reduce nodes and says both “reduce *r1*” and “reduce *r2*.” The state records
exactly the set of actions that a conventional table would place in the
conflicted cell.

Thus an LR conflict is the nondeterminism subset construction cannot and should
not erase: multiple semantic outputs survive in one deterministic subset. The
grammar, together with one token of evidence, has not selected a unique parse
action. Conflict resolution is the separate act of choosing an output.

Gazelle first reports these states, retaining their canonical item context for
diagnostics and counterexample generation. It then applies its default policy:

```text
items plus reduce nodes  -> item state       (shift wins)
several reduce nodes     -> lowest rule      (earlier rule wins)
one reduce node          -> that reduction
items only               -> item state
```

Other deterministic policies could replace this classifier. The relevant
property is timing: classification happens on the unmerged canonical machine.
After it, each reachable state has one runtime meaning.

A hybrid state can duplicate the item behavior of a pure item state elsewhere
in the automaton. This is an artifact of putting alternatives into target
states. Once classification discards the losing reduce nodes, ordinary
minimization merges such duplicates when their outgoing behavior agrees.

### 4.2 Why minimization alone is insufficient

Canonical same-core states often differ only because their reductions are
defined on disjoint lookahead sets. Consider states `q1` and `q2` that both
reduce by *r*, with `q1` defining the reduction on `a` and `q2` defining it on
`b`. Their valid continuations may be identical even though their partial
transition rows differ:

```text
       a       b
q1    R_r     error
q2    error   R_r
```

A minimizer correctly keeps these rows apart. Yet completing both gaps with
`R_r` is harmless when `b` cannot occur at `q1` and `a` cannot occur at `q2`
on a valid parse. Completion makes the latent equivalence visible:

```text
       a       b
q1    R_r     R_r
q2    R_r     R_r
```

The added entries may cause reductions on invalid input. If the reduction
eventually reaches an accepting continuation, however, the supposedly
impossible lookahead would witness a valid continuation of the original
state, contradicting the condition under which the gap was filled. Thus the
new edge can postpone rejection but cannot create an accepted sentence.

### 4.3 Conservative completion

Gazelle groups resolved item states by LR(0) core. Within a group, for every
terminal *a*, it inspects the reduce targets already present:

1. If every state defining a reduce transition on *a* agrees on the same
   target `R_r`, add that edge to siblings where *a* is absent.
2. If two states reduce different productions on *a*, leave every gap intact.
3. Never replace an existing transition.

The disagreement rule preserves the split in the LR(1)-but-not-LALR example
of §2.1. There, the same terminal selects different reductions in the two
states, so completion cannot make their rows equal.

The procedure is intentionally conservative. It does not search all possible
ways to complete error entries, nor does it claim that its choice produces the
smallest possible parser. It chooses a deterministic completion justified by
same-core context and unanimous existing reductions.

After completion, Gazelle runs iterative partition refinement. The initial
partition places all item states together and places reduce states in classes
distinguished by production. Refinement repeatedly splits a class when two
members transition, on some label, into different current classes. At the
fixed point, quotienting by the partition produces the coarsest transition-
preserving merge of the completed machine. The implementation is Moore-style
iterative refinement despite its historical function name
`hopcroft_minimize`.

### 4.4 Preservation argument

**Proposition 2 (resolved-behavior preservation).** Given the completion rule
of §4.3 and a deterministic classification of every canonical conflict, the
quotient parser accepts exactly the strings accepted by the resolved canonical
parser and performs the same shifts and reductions on each such string.

There are two transformations to justify.

**Completion.** Existing edges are never changed. On every valid canonical
parse, the current terminal or nonterminal therefore follows the same edge as
before, and the sequence of shifts and reductions is unchanged. A newly added
edge can be taken only where the original machine had an error. Because it is
a reduction agreed upon by the same-core contexts in which that lookahead is
defined, it can remove stack symbols and continue, but it cannot supply a
missing viable-prefix transition. Any run that later rejoins a valid accepting
path would imply that the lookahead was viable in the uncompleted state. The
added edge consequently affects only doomed runs.

**Quotienting.** Partition refinement merges only states with the same
classification and the same labeled transitions modulo the final partition.
Replacing a state by its equivalence class therefore preserves every future
edge and action of the completed machine. Reduce states for different
productions begin in distinct classes and can never merge. By induction over
parser steps—including the goto following each reduction—the quotient parser
has the same behavior as the completed parser.

Together, the transformations preserve accepted strings and the action trace
on every accepted string. They may change the point at which a rejected string
fails, as allowed by §2.2.

This is a proof sketch of the implemented criterion, not a claim that every
possible completion satisfying the same external semantics has been
characterized. A mechanized proof or an executable bisimulation checker would
strengthen the result.

### 4.5 Relation to NP-hard minimization

Optimal merging of LR(1) states is NP-hard because undefined entries provide
choices: different completions make different pairs compatible, and
compatibility need not be transitive [5]. Gazelle does not solve that search
problem. It first fixes the undefined entries using the unanimous-reduction
rule. Behavioral equivalence of the resulting labeled transition system is
then transitive and has a unique coarsest quotient computable in polynomial
time.

The word “minimize” in this paper always refers to this fixed completed
machine. It does not mean globally fewest states among all sound LR tables.

## 5. Runtime precedence as an application

Some conflicts are intentional. An expression rule such as

```text
expr = expr OP expr | atom
```

leaves associativity and precedence to a language policy. Traditional
generators resolve each conflicted table cell statically. Gazelle can instead
defer selected shift/reduce choices to token data, allowing one terminal to
represent operators whose precedence is known only at runtime.

For a precedence-bearing terminal `OP`, construction creates a virtual symbol
`OP_reduce`. Shift transitions retain the real symbol; completed items use the
virtual symbol on edges to reduce states:

```text
item --OP--------> item target
item --OP_reduce-> reduce target
```

Because the labels differ, subset construction and minimization see an
ordinary deterministic automaton. During table extraction the two columns are
combined into a `shift-or-reduce` entry. At runtime the incoming token's
precedence and associativity choose one branch.

Completion needs one additional guard. A state may lack `OP_reduce` while
having a real `OP` shift. Filling the virtual gap would not merely add a
reduction on an invalid path; it would turn a canonical unconditional shift
into a deferred choice on valid input. Gazelle therefore does not fill a
virtual reduce edge into a state that has a transition on its real twin.

The application illustrates the advantage of resolving semantics before
merging. The transition system can preserve both branches until runtime, and
partition refinement keeps apart exactly the states whose labeled choices
differ. A full treatment of runtime precedence—including its interaction with
lexer feedback and semantic values—is outside this paper's central claim.

## 6. Evaluation

Gazelle's `--yacc` mode emits an equivalent Bison grammar, allowing Bison to
serve as an independent implementation reference. The evaluation uses five
grammars: C++, C11, Python, Gazelle's regular-expression grammar, and its
self-hosted meta grammar. Precedence and conflict terminal modifiers are
stripped for the comparison so both tools receive the same bare grammar and
default conflict policy.

### 6.1 Canonical construction

The first comparison uses `bison -Dlr.type=canonical-lr`. Bison contributes
one synthetic `$accept` state; the table reports its count minus that state.

| grammar | Gazelle canonical item sets | Bison canonical − `$accept` |
|---------|----------------------------:|-----------------------------:|
| C++     | 5,350                       | 5,350                        |
| C11     | 2,097                       | 2,097                        |
| Python  | 3,298                       | 3,298                        |
| regex   | 69                          | 69                           |
| meta    | 61                          | 61                           |

Conflict counts also agree after accounting for Gazelle's hybrid-state
encoding. C11 has 128 shift/reduce and three reduce/reduce conflicts in both
tools; Python has 1,755 shift/reduce conflicts; regex has three; and meta has
none. The C++ grammar produces 3,182 physical hybrid conflicts in Gazelle
versus 2,893 Bison table cells. Deduplicating Gazelle conflicts by canonical
item set and terminal yields 2,893. The surplus consists of different
physical target states representing the same conventional conflicted cell.

These results test counts, not structural identity. They are consistent with
the correspondence in §3.1, but “same number of states” is weaker than a
bijection between item sets. A stronger test would export both machines and
compare normalized item sets directly.

### 6.2 Completed and minimized construction

The second comparison uses Bison's IELR and LALR modes, again subtracting the
synthetic accept state.

| grammar | Gazelle final | Bison IELR − `$accept` | Bison LALR − `$accept` |
|---------|--------------:|------------------------:|------------------------:|
| C++     | 601           | 601                     | 571                     |
| C11     | 470           | 470                     | 470                     |
| Python  | 418           | 418                     | 418                     |
| regex   | 44            | 44                      | 44                      |
| meta    | 61            | 61                      | 61                      |

Four evaluated grammars require no states beyond LALR. The C++ grammar is the
interesting case: IELR retains 30 more states than LALR to preserve canonical
resolved behavior, and Gazelle retains the same number. Gazelle compresses the
canonical machine by a factor of 8.9 while avoiding the merge that would alter
the reference parser.

The equality is empirical. It does not establish that Gazelle and IELR always
produce the same quotient, that the machines are isomorphic, or that either
count is globally optimal. The present evidence supports two separate claims:
the construction preserves canonical resolved behavior by its ordering and
equivalence criterion, and its compactness matches IELR on these grammars.

### 6.3 Implementation size and cost

The generic automaton module contains the NFA and DFA representations, subset
construction, iterative partition refinement, and column equivalence used by
both lexers and parsers. It is under 300 lines in the current implementation.
LR-specific code constructs the item NFA, classifies conflicts, completes
reduce edges, and extracts tables.

Gazelle eagerly allocates one NFA node for every `(production, dot,
lookahead)` triple, including unreachable triples. This trades memory for
simple index arithmetic; subset construction visits only reachable sets. The
C++ grammar builds in approximately six seconds in the current development
environment, including counterexample generation for roughly three thousand
intentional conflicts. These figures are engineering observations rather than
a controlled performance study. A complete evaluation should report peak
memory, construction time by phase, generated-table bytes, and parse speed
against other generators.

## 7. Related work

Knuth established canonical LR(k) parsing and the regularity of viable
prefixes [1]. The item-NFA account of closure and goto is established in
parsing texts and lecture treatments [6, 7]. Gazelle's contribution is not
subset construction itself, but placing reductions in the same labeled
transition relation so that later automaton algorithms see the complete
resolved table.

DeRemer's LALR construction merges states with equal LR(0) cores [2]. Pager
introduced compatibility tests that permit many such merges without creating
conflicts for LR grammars [3]. IELR starts from the compact LALR structure and
eliminates inadequacies—places where merging would change the behavior of the
canonical parser after conflict resolution [4]. Gazelle targets the same
behavioral baseline from the other direction: materialize and resolve the
canonical machine, then quotient its completed behavior.

Kannapinn gives the closest prior construction [9]. He builds completed
canonical LR(k) machines and applies partition refinement using right-context
and reduction information as Moore-style state output. He also discusses an
equivalent Mealy formulation. Crucially, he observes that ordinary
minimization of the bare canonical transition graph is insufficient because
reduction information is absent from that graph. Per-production reduce states
remove precisely this obstacle. Kannapinn assumes conflict-free LR(k)
grammars; canonical conflict resolution before minimization, conservative
completion, and runtime precedence are not part of that construction.

Reduction-incorporated generalized LR parsing also moves reductions into an
automaton, but with a different representation and purpose. Scott and
Johnstone connect reducing states to goto targets to reduce stack activity in
generalized parsing [10]. Gazelle instead uses lookahead-labeled edges into
rule-distinguished sink states so reductions participate in table
minimization. Heilbrunner's parsing-automata treatment belongs to the broader
automata-theoretic LR tradition but retains reduction information outside the
bare transition relation [8].

Yang proves that minimizing LR(1) state machines is NP-hard [5]. As §4.5
explains, Gazelle computes a behavioral quotient only after choosing a fixed
completion; it does not optimize over all sound completions and therefore does
not contradict that result.

## 8. Limitations and future work

The present construction has five important limitations.

First, the preservation argument should be made fully formal. In particular,
the safety of completion is expressed here as an invariant of viable
lookaheads and same-core states; a proof over parser configurations would make
all stack assumptions explicit.

Second, the evaluation establishes count agreement, not table equivalence.
Exporting Bison and Gazelle machines into a common representation would permit
item-set comparison for canonical construction and a bisimulation or product-
machine check after resolution.

Third, only five grammars are measured. A corpus covering more LR(1)-but-not-
LALR grammars, grammars with different conflict policies, nullable cycles, and
large generated languages would better characterize when Gazelle's fixed
completion matches IELR size and when it does not.

Fourth, spurious reductions weaken immediate error detection and can execute
semantic actions on invalid input. The generated parser preserves recognition
and valid parses, not an identical failed-parse trace. Error recovery may also
observe differences that simple recognition does not. These behaviors should
be measured explicitly.

Finally, the current partition-refinement implementation is a simple
Moore-style fixed-point algorithm, not Hopcroft's asymptotically faster
algorithm despite its function name. This does not affect the quotient it
computes, but the implementation and terminology should be brought into
agreement.

## 9. Conclusion

The hard part of compact LR tables is not merely deciding which states look
similar. It is preserving the meaning of conflicts while information is
discarded. Merge-first constructions must predict or repair that interaction.
Gazelle avoids it by changing both the order of operations and the
representation on which they operate.

Encoding `reduce r on a` as an edge labeled `a` into reduce state `R_r` makes
the complete parse table a labeled transition system. Subset construction
produces canonical LR(1) item sets and actions together. Conflicts are
reachable target states and are resolved while their canonical contexts are
still intact. Conservative completion exposes safe equivalences, and generic
partition refinement computes the coarsest quotient of the completed resolved
machine.

The result is not a solution to globally minimal LR table construction. It is
a simpler construction of a canonical-LR-equivalent parser whose table sizes,
on the evaluated grammars, equal those produced by IELR. More broadly, it is
an example of a useful engineering principle: when a domain-specific object
nearly fits a mature generic algorithm, changing the representation may be
more effective than inventing another domain-specific algorithm.

## References

[1] D. E. Knuth, “On the translation of languages from left to right,”
*Information and Control* 8(6), 1965.

[2] F. DeRemer, *Practical Translators for LR(k) Languages*, PhD thesis,
MIT, 1969.

[3] D. Pager, “A practical general method for constructing LR(k) parsers,”
*Acta Informatica* 7, 1977.

[4] J. E. Denny and B. A. Malloy, “The IELR(1) algorithm for generating
minimal LR(1) parser tables for non-LR(1) grammars with conflict resolution,”
*Science of Computer Programming* 75(11), 2010.

[5] Wuu Yang, “Minimizing LR(1) state machines is NP-hard,” arXiv:2110.00776,
2021.

[6] D. Grune and C. J. H. Jacobs, *Parsing Techniques: A Practical Guide*,
2nd ed., Springer, 2008.

[7] J. Gallier, notes on LR parsing and the item-NFA construction, University
of Pennsylvania.

[8] S. Heilbrunner, “A parsing automata approach to LR theory,” *Theoretical
Computer Science* 15, 1981.

[9] S. Kannapinn, *Eine Rekonstruktion der LR-Theorie zur Elimination von
Redundanz mit Anwendung auf den Bau von ELR-Parsern*, Dissertation,
Technische Universität Berlin, 2001.

[10] E. Scott and A. Johnstone, “Generalized bottom up parsers with reduced
stack activity,” *The Computer Journal* 48(5), 2005.
