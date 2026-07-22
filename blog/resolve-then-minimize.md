# Resolve, Complete, Then Minimize: LR Parse Tables as Automata

## Abstract

Canonical LR(1) construction preserves the context needed for correct conflict
resolution, but its tables often contain too many states to ship. Practical
generators therefore merge states early and then prevent or repair changes
caused by the merge. This paper explores the opposite order: construct the
canonical automaton, resolve conflicts there, complete selected error entries,
and minimize only the resulting behavior.

The key representation encodes `reduce r on a` as an ordinary transition
labeled `a` into a state distinguished by production *r*. Shifts, gotos, and
reductions then inhabit one labeled transition system. Subset construction
removes nondeterminism in *where the machine goes*, but a resulting subset may
still contain incompatible answers about *what the parser should do*: item and
reduce nodes together encode a shift/reduce conflict, while two reduce nodes
encode a reduce/reduce conflict. Resolution classifies this output
nondeterminism before context is discarded. A conservative completion then
aligns reductions across same-core states, and ordinary partition refinement
computes the quotient. Completion is the classical default-reduction
transformation under a per-terminal, alignment-driven selection policy. It may
delay errors but preserves accepted-input action traces.

Gazelle implements this pipeline using the same generic automaton module as
its lexer generator. On modifier-stripped versions of five grammars, its
canonical item-set and conflict counts agree with GNU Bison's canonical-LR
mode. After completion and minimization, its state counts agree with Bison's
IELR mode: the bare comparison form of a 292-line C++ grammar falls from 5,350
canonical states to 601, while LALR has 571 states and would change resolved
behavior. The production Gazelle grammar retains its terminal resolution
modifiers and has 632 states; the difference is the cost of preserving those
additional semantics. Equality of state counts is evidence about compactness,
not a proof that the constructions produce isomorphic machines or equally
sized encoded tables. The result is a simpler route to a
canonical-LR-equivalent parser whose state count, on the bare grammars
evaluated here, is the same as IELR.

## 1. Introduction

Canonical LR(1) gives a parser generator an attractive semantic baseline. Its
states retain the complete one-token right context derived from the grammar,
so a conflict is inherent in the grammar rather than introduced by table
construction. If a generator resolves conflicts in this automaton, the
resolution is applied in precisely the contexts in which each action arose.

The difficulty is size. The canonical automaton for the C++ grammar used in
this paper contains 5,350 item sets. The corresponding LALR automaton has
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
reaching an item state shifts, while reaching a reduce state *r* reduces by
production *r*. If determinization reaches both kinds at once, it has made the
transition deterministic without making the parser's answer deterministic:
that residual choice is the conflict. Reduce states are distinguished in the
initial partition during minimization.

This paper makes four contributions:

1. It gives an item-NFA encoding in which the complete LR table, including
   reductions, is one labeled transition system.
2. It identifies LR conflicts with output nondeterminism remaining in subset
   states after transition nondeterminism has been eliminated.
3. It shows that the classical default-reduction transformation, under a
   new alignment-driven selection policy, exposes common LALR-style merges
   to generic partition refinement while preserving valid-input parser
   behavior.
4. It evaluates state counts against Bison's canonical-LR and IELR modes and
   describes runtime precedence as an application of the representation.

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
Throughout, the grammar is assumed reduced (every symbol reachable and
productive) and non-cyclic (no `A =>+ A`) — the standard hypotheses under
which no LR parser can perform an unbounded cascade of reductions without
consuming input.

At runtime an LR parser keeps a stack of automaton states and holds one input
token in hand. A shift follows a terminal transition and pushes its target. A
reduction by `A -> beta` removes `|beta|` states, exposes a predecessor state,
and follows its transition on `A`. The lookahead token is not consumed by a
reduction; several reductions may occur before it is finally shifted. Two
standard terms recur below: a **viable prefix** is a grammar-symbol string
that can appear as the stack during some run extendable to acceptance, and a
**handle** is a top-of-stack right-hand-side occurrence that a correct
reduction removes — a property of the prefix, gated but not created by the
lookahead.

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
conflicts. Suppose a precedence declaration resolves a shift/reduce conflict
in favor of the reduction. A merge can union the conflicted context with a
same-core context in which the canonical automaton shifts unconditionally —
the completed item is present there too, but without that lookahead. The
merged cell holds both actions, the declaration selects the reduction, and
the reduction now also fires in the context whose canonical behavior was a
plain shift. Reduce/reduce resolution by rule order can migrate across
contexts the same way. Thus “the conflicts were resolved” is insufficient:
the merged parser may differ from the canonical parser under the same
resolution policy.

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

Gazelle adds one **reduce state** `R_r` for every production *r* — an accepting
state in the lexer sense. For every completed item `[A -> beta ., a]`, it adds

```text
[A -> beta ., a] --a--> R_r .
```

The reduce state is distinguished by production *r*: reaching it recognizes
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
    if target is a reduce state R_r for A -> beta:
      if r is the augmented start rule: accept
      pop |beta| states
      push delta(top(stack), A)
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
each reachable item-bearing subset state yields the canonical LR(1) item set
reached by the same symbol path, and every reachable canonical item set occurs
as such a projection. Pure reduce subsets represent action outputs and project
to the empty set. For every terminal and nonterminal, the encoded machine
contains the corresponding shift, goto, or reduce edge if and only if the
conventional canonical table contains that action. The item-bearing projection
is many-to-one: a hybrid state and a pure item state can share one item set, so
the encoded machine may carry more physical states than the canonical
automaton has item sets (§6.1 accounts for the duplicates when counting).

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

```text
shift item   --a-->  advanced item --+
                                      +--> subset {advanced item, R_r}
completed    --a-->  R_r -----------+
                           one edge, two parser outputs
```

**Figure 1.** Subset construction combines transition targets but does not
choose between their parser meanings.

Thus an LR conflict is the nondeterminism subset construction cannot and should
not erase: multiple semantic outputs survive in one deterministic subset. The
grammar, together with one token of evidence, has not selected a unique parse
action. Conflict resolution is the separate act of choosing an output.

Gazelle first reports these states, retaining their canonical item context for
diagnostics and counterexample generation. Classification is then driven not
by a global policy but by per-terminal declarations in the grammar. A
terminal has one of five kinds:

```text
plain           a conflict is an error, reported with counterexamples;
                it compiles only under a matching `expect` count
shift ELSE      shift/reduce on this terminal resolves to shift
reduce X        shift/reduce on this terminal resolves to reduce
prec OP         both actions are kept; runtime precedence decides (§5)
conflict NAME   both actions are kept; the lexer decides per token (§5)
```

There is no silent default. An unannotated conflict fails generation with
its counterexamples unless the grammar acknowledges the exact conflict
count (`expect 3 rr;`), in which case acknowledged reduce/reduce conflicts
resolve to the earlier rule; an acknowledged shift/reduce conflict resolves
to shift. Resolution is data in the grammar rather than an undocumented tool
default: the dangling else is the single declaration `shift ELSE`, naming the
classical disambiguation where the terminal is introduced. The property that
matters for this paper is timing: whatever the declarations say,
classification happens on the unmerged canonical machine. After it, each
reachable state has one runtime meaning — or, for `prec` and `conflict`
terminals, one precisely delimited deferred question (§5).

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

### 4.3 Completion as a default-reduction policy

The transformation in this step is not new. Replacing error entries by
reductions is the classical **default reduction**, used for LR table
compression since the earliest implementations [11]: a generator elects one
reduction of a state to stand for every unspecified lookahead, accepting
delayed error detection in exchange for compact rows. Bison performs exactly
this transformation under `%define lr.default-reduction` [12]. For the
pre-minimization completion, Gazelle changes only the *selection policy*.
Classical defaults choose one reduction per state, to compress that state's
row; Gazelle chooses per terminal, and aligns the choices across states with
the same LR(0) core, to make whole rows equal — compression of the state *set*
rather than of a single row.
Recognition and accepted-input action traces cannot tell these insertions
apart: in both, an error entry is replaced by a reduction that no successful
parse consults. Failed-parse behavior can distinguish them. The policies may
run different semantic actions on doomed paths, delay error reporting by
different amounts, and change when a feedback-sensitive lexer is called.

Under either policy, every inserted entry satisfies the same two local
conditions: (i) it replaces an error entry — no existing action changes —
and (ii) it extends the lookahead set of a reduction the state already
performs. Condition (ii) holds for Gazelle's rule because the donor sibling
has the same LR(0) core, so the donated reduction's completed item is
already present in the receiving state; completion never imports a
reduction from elsewhere. Classical per-state defaults are therefore a
special case of the insertions used here, and the preservation argument of
§4.4 covers both. One assumption deserves note: condition (i) reads an
error entry as evidence of non-viability, which is true of the canonical
machine under the policies of §4.1. A resolution policy that deliberately
maps *viable* lookaheads to error — yacc's `%nonassoc` — would create
error cells that this argument does not cover, and such cells must be
marked non-fillable, exactly as §5's virtual-twin guard already does for
deferred cells.

Concretely, Gazelle groups resolved item states by LR(0) core. Within a
group, for every terminal *a*, it inspects the reduce targets already
present:

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

The two selection policies are not rivals; Gazelle uses both, at different
stages. Alignment-driven insertion runs before minimization, where its job
is state-count reduction. The classical per-state default runs afterwards,
at table encoding: each state elects its most frequent reduction and the
encoded row omits the entries that match it — ordinary row compression on
the already-minimized machine.

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

**Completion.** Write a parser configuration as `(sigma, w)`, where `sigma` is
the stack of canonical states and `w` is the unread token string. Existing
edges are never changed. Every configuration on an accepting canonical run
therefore follows the same edge as before, and its sequence of shifts and
reductions is unchanged.

For rejection, consider the first inserted edge consulted in a run. It is a
reduction by `A -> beta` on lookahead `a` from a state whose canonical cell on
`a` was an error. The source state nevertheless contains the completed LR(0)
item `A -> beta .`; same-core completion has extended only its lookahead.
Let the stack encode a viable prefix ending in `gamma beta`. If taking the
inserted reduction and later accepting were possible, the accepting run would
witness a rightmost derivation in which `a` can follow the corresponding
occurrence of `A` after `beta` is reduced. Canonical LR(1) lookahead propagation
would then include `a` on that completed item and put a reduce action in the
original cell, contradicting that the cell was an error. Thus an inserted
reduction preserves the non-viability of the held lookahead. The repetition
is licensed by an invariant worth stating: the completed LR(0) item certifies
that `beta` is a handle independently of the held lookahead, so reducing it
preserves the viable-prefix invariant even though `a` is not valid there. The
argument therefore applies verbatim at the next inserted edge. A doomed run
never reaches a shift of the held token or an accepting configuration.

The parser may pop and reduce before discovering the error, but it rejects
without consuming the offending token: the correct-prefix property is
preserved. The argument is local to each inserted entry and run, so choosing
donors independently per terminal creates no interaction between insertions.
The reduced, non-cyclic hypothesis of §2 supplies termination: every inserted
edge is a locally legal reduction, and a no-input reduction cascade cannot be
unbounded. It must end in a state whose row has no entry for the held token.

**Quotienting.** Partition refinement merges only states with the same
classification and the same labeled transitions modulo the final partition.
Relate a completed-machine stack `q0 ... qn` to the quotient stack
`[q0] ... [qn]` pointwise. A shift preserves this relation because equivalent
states have transitions on the same label into the same target class. A
reduction preserves it because different rule states never merge, both stacks
pop the same number of entries, and equivalent exposed states have equivalent
gotos on the production's left-hand side. Induction over parser steps therefore
gives the same action at each configuration and preserves acceptance in both
directions.

Together, the transformations preserve accepted strings and the action trace
on every accepted string. They may change the point at which a rejected string
fails, as allowed by §2.2.

This remains a proof sketch of the implemented criterion, not a claim that
every completion satisfying the same external semantics has been
characterized. A full proof should define viability over stack configurations
and prove the inserted-reduction lemma above directly from the canonical item
construction. A mechanized proof or an executable equivalence checker would
strengthen the result further.

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
In this vocabulary the NP-hard search ranges over insertion policies; the
classical per-state default and the alignment policy used here are two
tractable points in that space.

## 5. Runtime precedence as an application

Some conflicts are intentional. An expression rule such as

```text
expr = expr OP expr | atom
```

leaves associativity and precedence to a language policy. Traditional
generators resolve each conflicted table cell statically. Gazelle can instead
defer selected shift/reduce choices to token data, allowing one terminal to
represent operators whose precedence is known only at runtime.

All non-plain terminal declarations use the same representational device.
For a modified terminal `OP`, construction creates a virtual symbol
`OP_reduce`. Shift transitions retain the real symbol; completed items use the
virtual symbol on edges to reduce states:

```text
item --OP--------> item target
item --OP_reduce-> reduce target
```

Because the labels differ, subset construction and minimization see an
ordinary deterministic automaton. During table extraction, `shift` and
`reduce` declarations select their named branch statically. The `prec` and
`conflict` declarations combine the two columns into a `shift-or-reduce`
entry. For `prec`, the incoming token's precedence and associativity choose
one branch at runtime.

Completion needs one additional guard. A state may lack `OP_reduce` while
having a real `OP` shift. Filling the virtual gap would not merely add a
reduction on an invalid path; it would turn a canonical unconditional shift
into a deferred choice on valid input. Gazelle therefore does not fill a
virtual reduce edge into a state that has a transition on its real twin.

`conflict` terminals use the same mechanism with a different decision source:
both branches survive to the table, and the lexer, rather than a precedence
comparison, supplies the answer per token. Gazelle uses this explicit feedback
channel for C's typedef ambiguity; the contextual decision remains in the
lexer, but its effect on parsing is represented directly in the table.

The application illustrates the advantage of resolving semantics before
merging. The transition system can preserve both branches until runtime, and
partition refinement keeps apart exactly the states whose labeled choices
differ. A full treatment of runtime precedence—including its interaction with
lexer feedback and semantic values—is outside this paper's central claim.

## 6. Evaluation

Gazelle's `--yacc` mode emits an equivalent Bison grammar, allowing Bison to
serve as an independent implementation reference. The evaluation uses five
grammars: C++, C11, Python, Gazelle's regular-expression grammar, and its
self-hosted meta grammar. Terminal resolution modifiers (`shift`, `reduce`,
`prec`, `conflict`) are stripped for the comparison, so both tools receive the
same bare grammar, and conflicts are counted before applying the tools'
matching default choices. These comparison machines are not the production
Gazelle tables when a grammar uses modifiers; §6.2 reports both counts
separately.

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

### 6.2 Completed and minimized state counts

The second comparison uses Bison's IELR and LALR modes, again subtracting the
synthetic accept state.

| bare grammar | Gazelle final | Bison IELR − `$accept` | Bison LALR − `$accept` |
|--------------|--------------:|------------------------:|------------------------:|
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

The production Gazelle grammars retain their terminal modifiers. Virtual
reduce symbols and the guard of §5 can keep states separate when merging them
would manufacture a static or runtime choice absent from the canonical
machine:

| production grammar | Gazelle states with modifiers |
|--------------------|-------------------------------:|
| C++                | 632                            |
| C11                | 506                            |
| Python             | 418                            |
| regex              | 44                             |
| meta               | 61                             |

The 31-state C++ and 36-state C11 differences are therefore not failures to
reach the bare IELR count. They are the measured cost of semantics removed
from the Bison comparison. Conversely, the equal Python, regex, and meta
counts show that modifiers do not necessarily prevent the same quotient.

The bare-count equality is empirical. It does not establish that Gazelle and
IELR always produce the same quotient, that the machines are isomorphic, that
their encoded tables occupy the same number of bytes, or that either count is
globally optimal. The present evidence supports two separate claims: the
construction preserves canonical resolved behavior by its ordering and
equivalence criterion, and its state-count compactness matches IELR on these
bare grammars.

### 6.3 Implementation size and cost

The generic automaton module contains the NFA and DFA representations, subset
construction, iterative partition refinement, and column equivalence used by
both lexers and parsers. It is under 300 lines in the current implementation.
LR-specific code constructs the item NFA, classifies conflicts, completes
reduce edges, and extracts tables.

Gazelle eagerly allocates one NFA node for every `(production, dot,
lookahead)` triple, including unreachable triples. This trades memory for
simple index arithmetic; subset construction visits only reachable sets. The
C++ grammar builds in approximately six seconds in the development environment
used for the reported comparison, including counterexample generation for
roughly three thousand intentional conflicts. The Bison measurements were
reproduced with GNU Bison 3.8.2. These figures are engineering observations
rather than a controlled performance study. A complete evaluation should
report the source revision and grammar hashes, hardware, peak memory,
construction time by phase, action and goto entry counts, generated-table
bytes, and parse speed against other generators.

The repository's opt-in Bison regression currently covers C11, Python, regex,
meta, and a small LR(1)-but-not-LALR witness. The larger C++ row and its
conflict normalization were measured separately; adding them to the automated
artifact is necessary for full reproducibility of the headline result.

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

Default reductions are long-standing table-compression practice, documented
in the early LR literature [11] and implemented in Bison [12], together with
the delayed error detection they cause. Their classical use compresses one
state's row. §4.3 reuses the identical transformation with a selection
policy chosen instead to align rows across same-core states — which is what
makes the latent equivalence visible to partition refinement.

Yang proves that minimizing LR(1) state machines is NP-hard [5]. As §4.5
explains, Gazelle computes a behavioral quotient only after choosing a fixed
completion; it does not optimize over all sound completions and therefore does
not contradict that result.

## 8. Limitations and future work

The present construction and evaluation have six important limitations.

First, the preservation argument should be made fully formal. §4.4 reduces
the parser-specific content to a single lemma — inserted reductions preserve
non-viability of the lookahead — with everything after it inherited from
automata theory; a proof over parser configurations would make that lemma's
stack assumptions explicit, and should include the formal statement that
classical per-state default reductions satisfy the same insertion
conditions.

Second, the evaluation establishes state-count agreement, not table
equivalence or equal encoded size.
Exporting Bison and Gazelle machines into a common representation would permit
item-set comparison for canonical construction and a bisimulation or product-
machine check after resolution. Reporting action/goto entries and serialized
bytes would test the motivating claim about tables being small enough to ship.

Third, only five grammars are measured, and the headline C++ row is not yet in
the automated differential regression. A corpus covering more LR(1)-but-not-
LALR grammars, grammars with different conflict policies, nullable cycles, and
large generated languages would better characterize when Gazelle's fixed
completion matches IELR size and when it does not.

Fourth, spurious reductions weaken immediate error detection and can execute
semantic actions on invalid input. The generated parser preserves recognition
and valid parses, not an identical failed-parse trace. Error recovery may also
observe differences that simple recognition does not. These behaviors should
be measured explicitly.

Fifth, the modifier-stripped Bison comparison isolates the minimization
algorithm but does not measure the cost of Gazelle's full resolution model.
The production counts in §6.2 begin to expose that cost; trace-equivalence
tests over adversarial grammars should cover `shift`, `reduce`, `prec`, and
`conflict` policies directly.

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
a simpler construction of a canonical-LR-equivalent parser whose state counts,
on the modifier-stripped grammars evaluated here, equal those produced by
IELR. Production grammars with additional resolution semantics may retain more
states, and equal state counts do not imply equal encoded bytes. More broadly,
the construction illustrates a useful engineering principle: when a
domain-specific object nearly fits a mature generic algorithm, changing the
representation may be more effective than inventing another domain-specific
algorithm.

## References

[1] D. E. Knuth, “On the translation of languages from left to right,”
*Information and Control* 8(6), pp. 607–639, 1965.

[2] F. DeRemer, *Practical Translators for LR(k) Languages*, PhD thesis,
MIT, 1969.

[3] D. Pager, “A practical general method for constructing LR(k) parsers,”
*Acta Informatica* 7, pp. 249–268, 1977.

[4] J. E. Denny and B. A. Malloy, “The IELR(1) algorithm for generating
minimal LR(1) parser tables for non-LR(1) grammars with conflict resolution,”
*Science of Computer Programming* 75(11), pp. 943–979, 2010.
doi:10.1016/j.scico.2009.08.001.

[5] Wuu Yang, “Minimizing LR(1) state machines is NP-hard,” arXiv:2110.00776,
2021. doi:10.48550/arXiv.2110.00776.

[6] D. Grune and C. J. H. Jacobs, *Parsing Techniques: A Practical Guide*,
2nd ed., Springer, 2008.

[7] J. Gallier, “A Survey of LR-Parsing Methods: The Graph Method for Computing
Fixed Points; Computation of FIRST, FOLLOW, and LALR(1) Lookahead Sets,”
University of Pennsylvania, 2008, §11.

[8] S. Heilbrunner, “A parsing automata approach to LR theory,” *Theoretical
Computer Science* 15, pp. 117–157, 1981.

[9] S. Kannapinn, *Eine Rekonstruktion der LR-Theorie zur Elimination von
Redundanz mit Anwendung auf den Bau von ELR-Parsern*, Dissertation,
Technische Universität Berlin, 2001, especially ch. 4, pp. 39–54.
doi:10.14279/depositonce-276.

[10] E. Scott and A. Johnstone, “Generalized bottom up parsers with reduced
stack activity,” *The Computer Journal* 48(5), pp. 565–587, 2005.

[11] A. V. Aho and S. C. Johnson, “LR parsing,” *ACM Computing Surveys*
6(2), pp. 99–124, 1974.

[12] Free Software Foundation, *Bison: The Yacc-compatible Parser Generator*,
manual §§5.8.1–5.8.2, “LR Table Construction” and “Default Reductions”
(`%define lr.default-reduction`).
