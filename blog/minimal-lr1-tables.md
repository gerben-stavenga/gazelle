# The Parse Table Is Just an Automaton

*Minimal LR(1) tables from generic DFA algorithms — how one encoding decision
makes conflict resolution, table compression, and runtime precedence fall out
of subset construction and partition refinement.*

## Abstract

Canonical LR(1) is the gold standard for deterministic parsing, but its
automaton is too large to ship: a real C++ grammar produces over 5000 states
where LALR needs 571. The classical fixes — LALR's core merging, Pager's weak
compatibility, IELR's split-back repair — are specialized merging procedures
with subtle interactions between merging and conflict resolution, and the
general problem of merging LR(1) states without changing behavior is NP-hard
in its usual formulation.

Gazelle takes a different route. We first develop parsing as reinserting
the production parentheses a parse tree elides from the token stream — LL
parsers reconstruct the opening ones, LR parsers the closing ones. The
knowledge needed to place a closing parenthesis is regular: an NFA reading
the stack computes it, determinizing that NFA yields canonical LR(1), and
conflicts are exactly the placements that stay ambiguous. One encoding
decision — *reduce actions
are transitions to explicit reduce states* — then turns the entire parse
table into the transition function of a single DFA. After that, everything
is a textbook automaton algorithm: canonical LR(1) construction is subset
construction over an item NFA, conflict resolution is a per-state
classification, and table compression is ordinary DFA minimization after a
40-line alignment pass. The same generic `automaton.rs` module (under 300
lines) builds both the lexer and the parser. Operator precedence is deferred
to parse time by renaming a symbol before the pipeline and merging it back
after, leaving every stage untouched.

The result is validated against GNU Bison: the raw automaton reproduces
bison's canonical LR(1) states and conflicts *exactly* on five grammars
(C++, C11, Python, our regex and meta grammars), and the minimized automaton
reproduces bison's IELR(1) state counts exactly — including the 30 states
IELR splits beyond LALR on the C++ grammar. We have not found the
reduce-states-as-transitions encoding in the LR literature; the closest
work, Kannapinn's minimal-LR(k) construction, minimizes the completed
canonical machine with reduce information carried as state output — and
explicitly rejects minimizing the bare transition structure as unsound,
which is precisely the obstacle this encoding removes (§8).

## 1. Introduction

In theory, parsing was settled sixty years ago: LR is the most powerful
deterministic method there is — linear time, one token of lookahead,
every deterministic context-free language [1]. In practice, nearly every
production compiler ships a hand-written recursive-descent parser, the
textbook's *weaker* method. Anyone who has tried the classic tools knows
why. There is the extra build step. There is the grammar file, with
semantic actions spliced in as quoted code in a foreign syntax. There is
the inversion of control: the generated parser owns the main loop and
calls you, on its schedule, with its 1970s API. And above all there are
the conflicts: when the tool rejects a grammar, it reports the failure in
terms of its own internals — "shift/reduce conflict in state 217" — and
with LALR the conflict may not even be the grammar's fault, but an
artifact of a merging heuristic the user was never told existed.

Gazelle, our parser generator, was written against this list. Its design
philosophy is to expose the algorithms rather than to wrap them: the
pieces of LR parsing — construction, conflict analysis, table
compression — should be components a user can understand, pick up, and
compose into a solution for the task they actually have, not a framework
that solves a predetermined task that is never quite the one at hand.
Such a philosophy stands or falls with comprehensibility: you can only
hand someone an algorithm you can explain. This paper is that
explanation. It presents LR parsing as gazelle implements it, and the
whole construction grows out of one concrete picture, so we start there.

The picture states parsing as a self-contained algorithmic task. A parse
tree is just the token stream with labeled parentheses written into it,
one pair per production. For the input `1 + 2`, parsed as an addition of
two numbers:

```
(add (num 1 )num + (num 2 )num )add
```

Erasing the parentheses gives back the input. Parsing is the inverse:
given a plain stream of tokens, reinsert the parentheses.

Half of that job is free: the label on an opening parenthesis determines
its closing one and vice versa, so reconstructing either side alone is
enough. And the choice of side is precisely the split between the two
great parser families. To write the *opening* parenthesis `(add`, a
parser must name the production at its first token, before seeing any of
the body; that is LL, and it is why left recursion kills recursive
descent. To write the *closing* parenthesis `)add`, a parser may wait
until the production's last token, with the entire body already seen;
that is LR. LR is the more powerful method for the most ordinary of
reasons: it makes the same decision later, with more information in
hand.

So the LR question is: where can a closing parenthesis go? The
remarkable answer, due to Knuth [1], is that the knowledge required is
*regular*: the stacks from which a parse can still succeed — the viable
prefixes — form a regular language, so a plain finite automaton, no
stack of its own, can read the parse stack and answer "which production
can end here?". Determinizing that automaton is exactly the canonical
LR(1) construction. The framing also localizes failure: a grammar is
LR(1) when every placement question has a unique answer after one token
of lookahead, and a *conflict* is nothing more than a placement question
that stays ambiguous.

Conflicts are usually presented as defects in the grammar. Often they
are the opposite: the ambiguous grammar is the cleaner and more
understandable formulation. `expr = expr OP expr` says what an
expression is far more directly than the unambiguous precedence ladder
it must be rewritten into, and the dangling else is simplest stated
ambiguously with "else binds to the nearest if" said once, out loud.
The ambiguity is real, but it belongs to the language definition, not to
a grammar bug — and a parser generator should support saying it that
way.

Between canonical LR(1) and a usable tool stand two practical problems:
the canonical tables are far too large to ship, and real grammars have
conflicts. Sixty years of tooling answers both with special-purpose
machinery — merging heuristics, repair phases, precedence decisions
frozen into the table at generation time. Gazelle's contrast, and this
paper's subject, is that no special-purpose machinery is needed: encoded
the right way, the whole parse table is a single ordinary DFA, and the
textbook automaton algorithms do all the work — construction,
conflict handling, and compression to state counts matching the state of
the art. One generic automaton module, containing nothing
parser-specific, builds gazelle's lexer and its parser both.

The rest of the paper walks the pipeline. §2 develops the parenthesis
picture into a parser driven by an oracle; §3 replaces the oracle with
the stack-reading automaton and derives canonical LR(1); §4 treats
conflicts as states and resolution as classification; §5 shrinks the
table by lookahead alignment plus minimization; §6 defers precedence to
parse time; §7 validates the state counts and conflicts against bison;
§8 positions the construction against prior work, in particular
Kannapinn's dissertation, the closest anticipation we know of; §9
records engineering notes.

## 2. Parsing is reinserting parentheses

Take the standard expression grammar:

```
expr = expr PLUS term => add | term => term;
term = NUM => num | LPAREN expr RPAREN => paren;
```

A parse tree is a token stream with parentheses. Write the tree for
`NUM PLUS NUM` inline, one labeled pair per production instance:

```
(add (term (num NUM )num )term PLUS (num NUM )num )add
```

If input arrived in this form, parsing would be trivial — normal code, ten
lines, one stack:

```rust
for event in input {
    match event {
        Token(t) => stack.push(Tree::Leaf(t)),
        Open(r)  => { /* unused! */ }
        Close(r) => {
            let children = stack.split_off(stack.len() - rhs_len(r));
            stack.push(Tree::Node(r, children));
        }
    }
}
```

And the code exposes a redundancy: it never looks at `Open`. The label on a
parenthesis names the production, the production fixes the number of
children, so each side of the pair determines the other. Keep only one:

- **Opening only** — `(add (term (num NUM PLUS (num NUM` — every production
  announces itself *before* its body: a preorder serialization, read back
  by recursive descent. In order, the labels spell a leftmost derivation.
- **Closing only** — `NUM )num )term PLUS NUM )num )add` — every production
  announces itself *after* its body, and the loop above runs unchanged. In
  order, the labels spell a rightmost derivation in reverse.

Raw input has neither, and parsing is exactly the job of reinserting the
elided parentheses. A parser that reconstructs the opening ones must name
each production at its *first* token — that is **LL**. One that
reconstructs the closing ones names it at its *last* token, having seen the
whole body — that is **LR**. The closing-only reader's two events have
traditional names: copying a token to the stack is a **shift**, executing a
`)r` — pop the right-hand side, push the left — is a **reduce**:

```
NUM      stack: NUM
)num     stack: term              pop 1, push   (term → NUM)
)term    stack: expr              pop 1, push   (expr → term)
PLUS     stack: expr PLUS
NUM      stack: expr PLUS NUM
)num     stack: expr PLUS term
)add     stack: expr              pop 3, push   (expr → expr PLUS term)
```

The choice of side is not symmetric. On `NUM PLUS NUM PLUS …` the very
first opening parenthesis cannot be placed with any finite peek: whether
the stream starts `(add` or `(term` depends on whether a `PLUS` turns up
after the first operand — arbitrarily far ahead — and with nested
additions, on how *many* turn up. Left recursion is fatal to LL. The first
*closing* parenthesis — `)num` after the first `NUM` — is placeable on the
spot. Deciding at the end of a production instead of its start is a
strictly easier problem (every LL grammar is LR, not conversely), and the
entire craft of LR is placing closing parentheses correctly.

Not that placement is always obvious. In the trace above, after the second
`)num` the stack reads `expr PLUS term`, and *two* productions have their
right-hand side on top: `)add` (correct) and `)term` — a trap, leaving
`expr PLUS expr`, a stack no production ever closes; the parse is dead and
doesn't know it yet. So hand the parser an **oracle** that answers the one
question the input no longer does: *which production of `b` comes next?*
With it, parsing is a single recursive function — nothing but the tree
walk:

```rust
fn parse(r: Production, stack: &mut Vec<Tree>) {
    emit_open(r);                              // "(r" — the LL announcement
    for sym in rhs(r) {
        match sym {
            Terminal(t)    => stack.push(Tree::Leaf(expect(t))),
            NonTerminal(b) => parse(oracle(b), stack),
        }
    }
    emit_close(r);                             // ")r" — the LR announcement
    let children = stack.split_off(stack.len() - rhs_len(r));
    stack.push(Tree::Node(r, children));
}
```

Note what is passed in: the *production*, not the nonterminal — choosing
between productions is the oracle's job, done at the call site. And note
where the data lives: not in locals, but on one stack threaded through the
recursion. A token is pushed as it is read, and the close collects the
production's children off the top — the same `push` and `split_off` as the
reader's loop above. The recursion owns no data at all; the call structure
exists only to know `r` and to place the announcements. And since one side
of each parenthesis pair is redundant, the parser owes the world only one
of the two `emit`s. Which one defines the family: announce at entry,
before a single token of the body has been read — **LL**; announce at
exit, the entire body in hand — **LR**. Same function, same walk, same
stack, opposite ends.

"Replacing the oracle with computation" now carries a precise deadline:
each `oracle(b)` answer must be *derived* by the time it is announced. An
LL parser derives it at the production's first token, peeking at most k
ahead — `oracle(b)` becomes a dispatch on the peek, the function becomes
recursive descent, and every parser written by hand is exactly this. The
asymmetry from before reads straight off the code: for the left-recursive
`expr`, the entry announcement — `(add` or `(term`? — is underivable from
any finite peek, while the exit announcement falls due only after the
whole body: a later deadline, with all the evidence in.

But look at what LR's deferral does to the function. Mid-parse, *no active
call knows its own argument yet*: every activation on the call stack is
`parse(r)` for an `r` that will only be derived when that activation ends.
The recursion is real, but its arguments are IOUs — it cannot be run
top-down as written. And since the data all sits on the shared stack, an
activation owns nothing but a position: which productions are still
candidates, and how far along their right-hand sides it stands. LL buys
program structure by committing at the earliest possible moment; LR
commits at the last possible moment, and what its call stack becomes is
the subject of §3.

A truthful oracle makes `parse` correct and useless — it presupposes the
parse. Parsing is the art of computing the announcements, and the rest of
this paper computes LR's.

## 3. The oracle's knowledge is regular

Two mechanical rewrites of `parse` change nothing and reveal everything.
First, demote the oracle to a guess: `choose(b)` picks *any* production of
`b`, forking the run into one world per pick, and wrong worlds simply
die — `expect` fails, or input is left over. The input is valid iff some
world survives; clairvoyance traded for nondeterminism. Second, dissolve
the for loop into the recursion by moving its index into the parameters:

```rust
fn parse(r: Production, dot: usize, stack: &mut Vec<Tree>) {
    if dot == rhs_len(r) {
        let children = stack.split_off(stack.len() - dot);    // ")r" — close
        stack.push(Tree::Node(r, children));
        return;
    }
    match rhs(r)[dot] {
        Terminal(t)    => stack.push(Tree::Leaf(expect(t))),  // push what arrives
        NonTerminal(b) => parse(choose(b), 0, stack),         // descend on a guess
    }
    parse(r, dot + 1, stack)                                  // advance
}
```

The parameter pair `(r, dot)` — a production and a position inside it —
is a **dotted production**, or *item*, and it is the entire control state
of the parse. The item `expr → expr • PLUS term` reads: "parsing an
`add`; the left operand is on the stack; `PLUS` must come next." The call
stack is a stack of items, each suspended at its dot. In the world that
survives `NUM PLUS NUM`, the run looks like this:

```
[__start → •expr]                                  descend: add    (guess)
[__start → •expr │ expr → •expr PLUS term]         descend: term   (guess)
[… │ expr → •expr PLUS term │ expr → •term]        descend: num    (guess)
[… │ expr → •term │ term → •NUM]                   read NUM
[… │ expr → •term │ term → NUM•]                   close )num
[… │ expr → •expr PLUS term │ expr → term•]        close )term
[__start → •expr │ expr → expr •PLUS term]         read PLUS
[__start → •expr │ expr → expr PLUS •term]         descend: num    (guess)
[… │ expr → expr PLUS •term │ term → •NUM]         read NUM, close )num
[__start → •expr │ expr → expr PLUS term•]         close )add
[__start → expr•]                                  accept
```

Two things are visible in this form that the for loop hid.

**There is exactly one decision.** Whether the first branch fires — end
the loop, the production is complete, emit `)r`. That is the entire
decision content of LR parsing; everything else in the function is
bookkeeping.

**The not-complete path is state independent.** What it does is dictated
by what arrives: a token arrives and is pushed; a subtree is delivered by
an inner close and the dot steps over it. The parameters are consulted
only as a check — `rhs(r)[dot]` says what *may* arrive — never as a
choice; on valid input, every candidate item does the same pushing and
advancing. Only the complete branch reads `r` in earnest: the arity to
pop, the label to emit. Even a descent is anonymous while it lasts —
`parse(choose(b), 0, …)` makes a guess, but the guess is silent (no input
consumed, no parenthesis emitted) and is not *used* until the child's own
complete branch fires.

Now the NFA can be read straight off the code. One world's control state
is the pair `(r, dot)`, drawn from a finite set, and the three branches
change it in exactly three ways:

```
(r, dot) --X-->  (r, dot+1)    advance: the dot steps over X = rhs(r)[dot] —
                               a token, or a subtree delivered by a close
(r, dot) --ε-->  (b', 0)       descend: a guessed production of b — silent
(r, end)  close                return: the parent's dot steps over lhs(r) —
                               an ordinary advance, on a nonterminal
```

Finite control, symbol-labeled steps, silent guessed steps: the parser
*is* a **nondeterministic finite automaton** driving a stack. The
recursion holds no data (the trees are on the shared stack) and makes no
choices beyond the guesses; all it remembers is which items to resume.

Running a nondeterministic machine means running all its worlds — naively
exponential, but the two observations above collapse it. Until a close
fires, every world does the same pushing and advancing, so the worlds
march through the input together; all a world privately owns is its item
and its suspended items. The item ranges over a finite set — carry the
set. And the suspended items are recoverable from what all worlds share:
descents consume nothing, so every symbol any dot ever passed is on the
data stack, and a candidate configuration is exactly a way of
*re-reading* that stack from `__start → •expr` — the configuration
`[__start → •expr │ expr → expr PLUS •term │ term → •NUM]` is the NFA
path that reads `expr`, then `PLUS`, descending twice on the way. So one
scan carries every world: run the NFA over the data stack, bottom to top,
all paths at once, keeping the set of reachable items after each symbol.
The nondeterminism is swallowed by the sets — each set is a *function* of
the one before it — so the set-to-set machine is deterministic: the
**subset construction**, a DFA whose states are sets of items. (This is
Knuth's foundational observation [1]: the stacks from which a parse can
still succeed — the *viable prefixes* — form a regular language.)
Scanning `expr PLUS NUM`:

```
{ __start → •expr, expr → •expr PLUS term, expr → •term, term → •NUM, … }
   --expr-->  { __start → expr•, expr → expr •PLUS term }
   --PLUS-->  { expr → expr PLUS •term, term → •NUM, term → •LPAREN expr RPAREN }
   --NUM-->   { term → NUM• }
```

A close `)r` for `B → γ` is viable exactly when the scan reaches the
completed item `B → γ •`: the symbol edges traversed last put γ on top of
the stack, and the epsilon edge that entered `B → • γ` came from a
suspended parent whose dot stands before `B` — the two conditions of a
correct close, established by one reachability question. Call a reachable
completed item a **verdict**. Here there is one: `)num`, the unique viable
move. Scan the trap
stack `expr PLUS expr` instead and the third step has no edge on `expr` —
the set goes empty; nothing is viable, which is exactly what "dead" means,
detected by a regular scan.

The option list is not always a singleton: in the second set above,
`__start → expr•` is a verdict too, and it competes with shifting `PLUS`.
The tie-breaker is one token of peek, pushed *into* the NFA: annotate every
item with a **follow-token**, the token that must arrive after its
production. The epsilon edge from `A → α • B β` with follow-token `t`
spawns `B → • γ` with follow-tokens drawn from FIRST(β·t) — whatever can
actually follow this particular `B`. A completed item then delivers its
verdict only when the next input token is its follow-token. Item plus
follow-token is the classical LR(1) item; grammars for which the refined
option list is a singleton everywhere — shift, or one specific reduce — are
exactly **LR(1)**: the oracle replaced by a regular scan and one token of
peek.

Now make the verdict itself a state, because it earns its keep. Give every
production one extra NFA state — its **reduce node** — and let the
completed item step to it *on its follow-token*: one more ordinary labeled
edge. Accept stops being special; it is the reduce node of the augmented
`__start → expr`. The oracle-free parser is then the loop of §2 with the
oracle replaced by a single NFA step on the peeked token — the kind of
state the step lands in *is* the event:

```rust
loop {
    let live = scan(&stack);            // NFA states, reading the stack
    let next = step(&live, peek());     // one more step, on the lookahead
    if let Some(r) = next.reduce_node() {
        close(r);                       // ")r": pop rhs_len(r), push Node(r)
    } else if !next.is_empty() {
        shift();                        // push Leaf(tokens.next())
    } else {
        return Err(SyntaxError);
    }
}
```

Closing `__start` returns the finished tree; a close only peeks at the
token, which is consumed when its turn to shift comes. For an LR(1) grammar
exactly one branch applies at every step — the case where two apply is §4.

One inefficiency remains: rescanning the whole stack after every move is
quadratic, and wasteful in an obvious way — the scan of the untouched lower
stack cannot change. So cache it: alongside each stack symbol, store the
DFA state — the item set — the scan reaches at that point. A shift
computes the new top state in one DFA step; a reduce `B → γ` pops `|γ|`
symbols together with their cached states, exposing — still valid, never
recomputed — the state from before γ was matched, and pushes `B` with its
one-step update: the classical *goto*. The reachable item sets are
precisely the canonical LR(1) states, and at this point the grammar
symbols themselves are dead weight — every decision reads only the cached
states — so the textbook parser stops storing them. The loop keeps its
shape; `scan` has become a lookup. The mysterious "stack of LR states" is
a memoized NFA scan over a symbol stack that is no longer materialized.

### Reduce actions as transitions

The item half of this NFA is the textbook construction [6, 7]. The verdict
half — reduce nodes as ordinary states, reduce actions as ordinary edges —
is, as far as we can tell, absent from the literature (§8; Kannapinn [13]
comes closest, and explicitly dismisses the transition-only route), and it
is the load-bearing half: with it the transition function carries the *whole*
parser — symbol edges for shift and goto, follow-token edges for reduce —
and every algorithm that speaks DFA can from here on speak parser.

Gazelle builds the NFA by a triple loop over `(rule, dot, lookahead)`. Each
combination is one NFA state, laid out flat by index arithmetic — no
worklist, no interning:

```rust
let item_state = |rule, dot, la| rule_offsets[rule] + dot * num_terminals + la;

for (rule_idx, rule) in grammar.rules.iter().enumerate() {
    for dot in 0..=rule.rhs.len() {
        for la in 0..num_terminals {
            let idx = item_state(rule_idx, dot, la);
            if dot == rule.rhs.len() {
                // Completed item: transition on the follow-token to the reduce node.
                nfa.add_transition(idx, la, num_items + rule_idx);
            } else {
                // Shift/goto: advance the dot.
                nfa.add_transition(idx, rule.rhs[dot].id(), item_state(rule_idx, dot + 1, la));
                // Closure: epsilon edges to B's productions, follow-tokens from FIRST(β·la).
                if rule.rhs[dot].is_non_terminal() {
                    for (closure_rule, _) in grammar.rules_for(rule.rhs[dot]) {
                        for closure_la in first(&rule.rhs[dot + 1..], la) {
                            nfa.add_epsilon(idx, item_state(closure_rule, 0, closure_la));
                        }
                    }
                }
            }
        }
    }
}
```

Most enumerated items are unreachable; subset construction never visits
them, so enumerating eagerly costs nothing but a Vec. Reduce nodes are
sinks — no outgoing edges.

Determinize and read off what you get: each DFA state is a set of NFA
states — its items are exactly a canonical LR(1) item set, its reduce nodes
the verdicts delivered there — and where a classical generator fills a
separate action table, here the action row of a state is nothing but its
ordinary transition list.

So the whole parser is one transition function δ(state, symbol). States are
laid out with item states first, then one reduce state per rule; a target
below `num_item_states` means shift/goto, a target at
`num_item_states + r` means reduce rule *r*. That layout *is* the parse
table.

The claim that this equals Knuth's construction is checked empirically in
§7: distinct item sets match `bison -Dlr.type=canonical-lr` state-for-state
on all five test grammars.

## 4. The residual oracle: conflicts are states

For grammars that are not LR(1), the oracle is not fully eliminated: at
some points the machine, even peeking at the next token, still has more
than one proposed action. Where do those points live in the automaton?

The DFA is deterministic by construction — one transition per
(state, symbol). So a shift/reduce conflict cannot be a clash of edges; it
lands in the *target state*. If some item in state S shifts terminal `a`
and some completed item in S delivers its verdict on follow-token `a`, subset
construction builds a single transition on `a` to a **hybrid** target:
{advanced items} ∪ {reduce nodes} — a state on which both arms of §3's
loop match. A reduce/reduce conflict is a target containing two reduce
nodes. A conflict is not a table cell gone wrong; it
is a local, inspectable state — the state is literally *the set of answers
the oracle could still give*.

Resolution, in turn, is not table surgery. It is a policy for answering the
residual oracle questions, applied as a classification of each state, once:

```rust
// SR (items + reduce nodes): shift wins → the state is an item state.
// RR (multiple reduce nodes): lowest-numbered rule wins.
if !nfa_items.is_empty() {
    DfaStateKind::Items(items)
} else {
    reduces.sort();
    DfaStateKind::Reduce(reduces[0])
}
```

That is the entire conflict resolution engine — canned oracle answers,
twenty lines. It runs on the *canonical* automaton, before any merging — so
the interaction that IELR exists to repair (resolution acting on lookahead
sets contaminated by merging) cannot occur, by construction rather than by
repair.

The hybrid encoding has one artifact worth naming. A hybrid state has the
same items as some pure state elsewhere in the automaton, so the raw DFA
carries a duplicate physical state per conflicted transition. It is
self-healing: reduce nodes are sinks and classification declares the hybrid
an item state, so after resolution the hybrid and its pure twin have
identical behavior and the minimizer (§5) merges them. The only place the
duplicates are ever visible is in raw conflict counts, which can exceed
bison's per-(state, token) counts (§7).

Because a conflict is a state of a live automaton, generating a *witness*
for one — an input prefix reaching it, and completions showing the two runs
the machine cannot tell apart — is a graph search over δ rather than a
separate theory; that machinery is described elsewhere [9].

## 5. Small tables: align, then minimize

Canonical LR(1) for our C++ grammar has 5350 item sets; bison's LALR table
for the same grammar has 571 states (discounting bison's synthetic
`$accept` state, as in §7 throughout). Closing that gap is a fifty-year
literature, and nearly all its answers share one shape — build fewer states
by merging the ones that "probably" behave alike:

- **LALR** [2] merges every pair of states with the same LR(0) core and
  unions their lookahead sets. Usually fine; occasionally it manufactures a
  reduce/reduce conflict the canonical automaton doesn't have, and the
  grammar is rejected for no user-visible reason.
- **Pager's method** [3] merges lazily under a "weak compatibility" test,
  entangled with construction order.
- **IELR(1)** [4] starts from LALR, detects states where merging changed
  the *outcome of conflict resolution*, and splits exactly those back. This
  is the state of the art, implemented in Bison — and its motivation is
  telling: merging and resolution interact, and repairing the interaction
  requires tracking where every lookahead came from.
- In the general formulation — find a minimal merge of canonical states
  that introduces no conflicts — the problem is NP-hard [5].
- The one exception to the merge-during-construction shape is Kannapinn
  [13], who minimizes the *completed* canonical LR(k) machine by partition
  refinement — but with reduce information as state annotations rather
  than transitions, and only for conflict-free grammars (§8).

Gazelle closes the gap with two observations and no new algorithm.

**Observation 1: correctness constrains valid inputs only.** A parser must
accept every valid input with the right parse tree and reject every invalid
input. On valid inputs the sequence of shifts and reductions is fully
determined. On invalid inputs it owes you an error — *which* error, and how
much futile work precedes it, is unobservable to recognition. Two states
that differ only in behavior after the input has already gone wrong are
interchangeable.

Concretely: a canonical state reduces `E → e` only on lookahead `a`,
because on valid input nothing else can follow. Adding a reduce transition
on `b` as well changes nothing on valid inputs (the case never fires) and
on invalid inputs causes a **spurious reduction** — the parser reduces,
continues briefly, and reports the error a few steps later. Recognition is
preserved. (One honest caveat: reductions run semantic actions, so a failed
parse is not a rollback boundary for side effects in user actions —
spurious reductions preserve recognition and valid-input semantics, not
side-effect-freedom of doomed parses.)

**Observation 2: with reduces encoded as transitions, "interchangeable"
is DFA equivalence.** Adding spurious reductions is adding edges; deciding
which states can merge afterward needs no LR theory at all — a DFA
minimizer already computes exactly the coarsest merge that preserves
behavior.

The alignment pass, `merge_lookaheads`, is 40 lines: group item states by
LR(0) core, and within each group fill in each state's missing reduce
transitions from its siblings — but only where every sibling that *has* the
transition agrees on the target:

```rust
for &state in group {
    for &(sym, target) in &dfa.transitions[state] {
        if is_reduce(target) {
            sym_to_target.entry(sym)
                .and_modify(|t| if *t != Some(target) { *t = None })  // disagreement: leave the gap
                .or_insert(Some(target));
        }
    }
}
// then: add each agreed-on (sym, target) to every state in the group missing it
```

For the classic LR(1)-but-not-LALR grammar — state A reduces `E` on `a` and
`F` on `b`, state B reduces `E` on `b` and `F` on `a` — every symbol
disagrees, no gap is filled, the states keep distinct behavior, and
minimization correctly refuses to merge them. For the overwhelmingly common
case — same-core states reducing the same rule on disjoint lookaheads — the
gaps fill, the states become transition-identical, and they merge.

Then run partition refinement (Moore's algorithm; the code optimistically
names it `hopcroft_minimize`). The one LR-specific input is the initial
partition: reduce states are sinks, indistinguishable by transitions alone,
so they are pre-partitioned by rule; all item states start together. After
minimization the states are permuted — item states first, reduce state for
rule *r* at `num_item_states + r` — and that flat DFA is the shipped table.

The full pipeline, next to its sibling:

```
regex   → NFA → subset construction →                                  minimize → lexer DFA
grammar → NFA → subset construction → classify → resolve → align → minimize → parse table
```

The LR-specific code is at the boundaries: the triple loop before, and
classification/alignment/extraction after. Subset construction and
minimization are the same functions the lexer uses.

**Why this lands on IELR's size.** IELR's criterion is: merge same-core
states except where merging would change the outcome of conflict
resolution. Gazelle expresses the same criterion declaratively. Resolution
has already run (on canonical states, uncontaminated); alignment makes
same-core states identical precisely when their post-resolution behavior
agrees; the minimizer merges precisely the behaviorally identical states.
Where resolution outcomes differ, the transition functions differ, and the
split survives — not because an inadequacy detector found it, but because
partition refinement cannot do otherwise. No lookahead provenance, no
annotation propagation, no repair phase. This is an argument that the two
criteria coincide, not a machine-checked equivalence of the resulting
tables; what §7 verifies is that the state counts coincide exactly,
including the hard case.

This also explains why the NP-hardness of optimal LR(1) merging [5] is not
in our way. That formulation searches for a minimal conflict-free merge of
canonical tables, where merging can *create* conflicts and the search must
avoid them. Gazelle never searches: it first canonicalizes behavior
(resolve, then align — both deterministic), then computes *the* coarsest
behavior-preserving merge, which is unique and cheap. The price is
philosophical, not practical: we minimize behavior after fixing a
resolution policy, rather than minimizing over all conflict-free tables.

## 6. Runtime precedence: consulting the oracle at parse time

Expression grammars are the one place conflicts are not bugs but language
definition: `expr = expr OP expr` is ambiguous until precedence and
associativity say which way `1 + 2 * 3` parses. In oracle terms, the
question is genuine — the grammar does not determine the answer; the
*language* does. Yacc answers it statically with `%left`/`%right`: each
conflicted cell is fixed to shift or reduce at generation time. That
forecloses anything the table didn't anticipate: every operator must be a
distinct terminal with a declared level, and user-defined operators require
regenerating the parser.

Gazelle instead ships the unanswered question in the table and puts the
oracle back at parse time — where it is no oracle at all, just data on the
token. The mechanism is one symbol rename. A terminal marked `prec` gets a
*virtual* twin: completed items transition to reduce nodes on the virtual
id, while shifts use the real id.

```rust
let sym = prec_to_reduce[la].unwrap_or(la);   // virtual twin for prec terminals
nfa.add_transition(idx, sym, reduce_node);
```

Shift-on-real and reduce-on-virtual are transitions on *different symbols*,
so the DFA is deterministic and no stage sees a conflict: detection skips
virtual symbols, resolution finds nothing to resolve, and minimization
treats them as ordinary labels — a state with both edges has a different
signature than a state with one, so partition refinement keeps exactly the
distinctions that matter. After minimization, table extraction
merges the twins back: a state with a shift edge on real `OP` and a reduce
edge on virtual `OP` yields a `ShiftOrReduce { shift_state, reduce_rule }`
entry carrying *both* options.

At parse time, hitting `ShiftOrReduce` triggers a fifteen-line comparison
between the precedence of the operator on the stack and the precedence
carried by the incoming token: higher incoming level shifts, lower reduces,
equal level resolves by associativity. This is Dijkstra's shunting-yard
[8] running on the LR stack itself — the parse stack is the operator
stack — except it is not a separate expression-parsing island: the same
automaton is parsing statements, declarations, and types, and shunting-yard
behavior switches on exactly at the `ShiftOrReduce` entries.

One stage, though, must know about the twins: alignment. The gap-filling
argument of §5 rests on an invariant — a state with no transition on a
symbol can never see that symbol on valid input, so filling the gap with a
spurious reduce is unobservable. Virtual twins break the invariant: a state
can lack the *virtual* reduce edge while shifting the *real* token, because
the canonical automaton shifts unconditionally there. Filling that gap does
not add an error-path reduction; it manufactures a `ShiftOrReduce` entry in
a state where canonical LR(1) has no question to defer, and the token's
precedence can then drive the parser off a valid input. The guard is one
condition: never fill a virtual reduce edge into a state that has a
transition on its real twin. It is not free — on the C11 grammar (with its
`prec` operators; the §7 comparison strips them, so no virtual symbols
exist there) the guard keeps 36 states apart that unguarded filling would
have merged — but every one of those merges was only reachable by
contaminating a genuinely unconditional shift context with another
context's deferred question.

Note what static resolution *cannot* express here: with one grammar rule
for all binary operators, the same table cell must shift for `*` and reduce
for `+`. A cell fixed at generation time picks one. `ShiftOrReduce` keeps
both transitions through minimization and lets the token decide. Precedence
becomes data flowing through a fixed table — which is why user-defined
operators cost nothing: the lexer tags `@` with `Left(14)` and the parser
has never heard of `@`.

One rule, one table entry, fifteen precedence levels, user-defined
operators. The C11 grammar drops from a fifteen-nonterminal precedence
ladder to a single rule.

## 7. Validation against Bison

Claims of "exactly canonical LR(1)" and "exactly IELR-sized" should not
rest on inspection. Gazelle has a `--yacc` mode that emits a grammar as a
`.y` file, which gives us bison as an independent oracle. For the
comparison, `prec`/`shift` terminal modifiers are stripped from the gazelle
grammars (bison receives them as plain tokens, so both tools must see the
same bare grammar). Five grammars: a 292-line C++ grammar, C11, Python,
gazelle's regex grammar, and gazelle's self-hosted meta grammar.

**Raw automaton vs `bison -Dlr.type=canonical-lr`.** Bison adds one
synthetic state for its `$accept` rule; discounting it, gazelle's distinct
LR(1) item sets match bison's canonical states exactly:

| grammar | gazelle item sets | bison canonical − $accept |
|---------|------------------:|--------------------------:|
| C++     | 5350              | 5350                       |
| C11     | 2097              | 2097                       |
| Python  | 3298              | 3298                       |
| regex   | 69                | 69                         |
| meta    | 61                | 61                         |

Conflicts match as well: C11 shows 128 shift/reduce and 3 reduce/reduce in
both tools, Python 1755 S/R, regex 3 S/R, meta none. The C++ grammar
exposes the hybrid-state artifact from §4: gazelle's raw count is 3182 S/R
against bison's 2893, and deduplicating gazelle's by (item set, token)
yields exactly 2893 — the surplus is duplicate physical states, not extra
conflicts.

**Minimized automaton vs `bison -Dlr.type=ielr`.** Same $accept discount:

| grammar | gazelle final | bison IELR − $accept | bison LALR − $accept |
|---------|--------------:|---------------------:|---------------------:|
| C++     | 601           | 601                  | 571                  |
| C11     | 470           | 470                  | 470                  |
| Python  | 418           | 418                  | 418                  |
| regex   | 44            | 44                   | 44                   |
| meta    | 61            | 61                   | 61                   |

Four grammars are LALR, and gazelle lands on the LALR count — alignment
plus minimization gives up nothing. The C++ grammar is the decisive case:
IELR splits 30 states beyond LALR to keep conflict resolution faithful to
canonical behavior, and gazelle reproduces the split exactly — from a
40-line gap-filler and a generic minimizer, with no inadequacy analysis.
(This is a count-level comparison; the full certificate would be a
bisimulation of the product automaton, which the encoding makes
straightforward future work.) The 5350 → 601 compression on C++, an 8.9×
reduction landing precisely on the IELR optimum, is the paper's claim in
one number.

## 8. Related work

The item-NFA view of LR construction is folklore-old and well documented
[6, 7]: LR(1) states are the subset construction of an NFA over items, with
closure as epsilon edges. To our knowledge the second half of gazelle's
encoding — reduce actions as transitions to per-rule reduce states, making
the *entire* table one transition function — does not appear in the
textbooks or the state-reduction literature, and it is the enabling move:
without it, "reduce E on {a}" and "reduce E on {b}" are annotations that no
generic algorithm can align or compare, and the field's answers (LALR [2],
Pager [3], Honalee [10], IELR [4]) are all bespoke merging procedures over
annotated item sets.

The closest prior art is Kannapinn's dissertation [13], which constructs
minimal general LR(k) parsers by exactly the post-hoc route: build the
completed canonical machine, then run Hopcroft/Gries partition refinement
seeded with a partition derived from the right-context (reduce)
information, treated as Moore-style state *output*. He even notes the
equivalent Mealy formulation — right-context attached to transitions — and
proves both minimize to structurally identical machines. What he does not
do is make reduce actions ordinary labeled transitions: he raises the idea
of applying standard DFA minimization to the bare canonical machine and
explicitly rejects it, because the reduce information is absent from the
transition structure. That rejected idea is this paper's starting point —
per-rule reduce states put the missing information *into* the transition
structure, after which the generic minimizer needs no LR-aware seeding
beyond pre-partitioning the sink states by rule. Kannapinn also assumes a
conflict-free LR(k) grammar throughout: resolution before minimization,
the alignment pass, runtime precedence surviving the pipeline, and the
IELR-size results for non-LR(1) grammars have no counterpart there.

Reduction-incorporated parsing (Scott and Johnstone's RIGLR [14]) also
folds reductions into the automaton, but as edges connecting a reducing
state directly to its goto target, and to a different end: eliminating
stack activity for non-embedded recursion in generalized (GLR-family)
parsing. Neither the encoding (no follow-token-labeled edges into per-rule
sink states) nor the goal (runtime speed, not table minimization) is
gazelle's. Heilbrunner's automaton-theoretic treatment of LR theory via
item grammars and parsing automata [11] is the tradition this paper works
in; it, too, keeps reduce actions out of the transition relation. Yang [5]
shows the general minimal-conflict-free-merge problem is NP-hard, which
locates the difficulty gazelle sidesteps (§5). Zimmerman's langcc [12]
represents the other contemporary full-LR(1) effort; it deliberately avoids
constructing the canonical automaton, attacking table size with grammar
transformations (CPS) and construction-time follow-set partitioning rather
than post-hoc minimization — the approaches compose rather than compete.

## 9. Engineering notes

The factorization is visible in the module sizes. `automaton.rs` — Nfa,
Dfa, subset construction, partition-refinement minimization, plus a
symbol-equivalence-class pass that compresses table *columns* the same way
minimization compresses rows — is 287 lines, and is the complete overlap
between the lexer and parser generators. The LR-specific side (grammar
lowering, FIRST sets, the triple loop, classification, alignment,
extraction) is about 1200 lines; conflict diagnostics (detection,
counterexample search) another file. Everything is `no_std` + `alloc`.

Eagerly enumerating all (rule, dot, lookahead) items looks profligate —
most are unreachable — but it replaces interning and worklists with index
arithmetic, and subset construction only ever touches the reachable ones.
The C++ grammar, the largest we have, builds in ~6 seconds including
counterexample generation for its ~3000 intentional conflicts; the
automaton pipeline itself is a small fraction of that.

## 10. Conclusion

Choose the representation in which your hardest problem is a solved one.
Presenting parsing as reinserting elided parentheses — and LR as placing
the closing ones, the latest possible moment to decide — locates every
difficulty at once: the placements that stay ambiguous are conflicts,
resolution policies are canned answers, and runtime precedence is the one
question honest enough to be sent back to the data. Encoding the machine so that reduce actions are
transitions costs one extra NFA state per rule and buys: canonical LR(1) as
bare subset construction, conflicts as inspectable states, resolution as a
twenty-line classification, IELR-exact table compression as generic
minimization behind a 40-line alignment pass, and precedence as a symbol
rename at one boundary and a merge at the other. Nothing in the middle of
the pipeline knows what a parser is — which is exactly why all of it can be
reused, tested, and trusted independently, and why the lexer generator is
the same code.

## References

[1] D. E. Knuth, "On the translation of languages from left to right,"
*Information and Control* 8(6), 1965.

[2] F. DeRemer, "Practical translators for LR(k) languages," PhD thesis,
MIT, 1969.

[3] D. Pager, "A practical general method for constructing LR(k) parsers,"
*Acta Informatica* 7, 1977.

[4] J. E. Denny and B. A. Malloy, "The IELR(1) algorithm for generating
minimal LR(1) parser tables for non-LR(1) grammars with conflict
resolution," *Science of Computer Programming* 75(11), 2010.

[5] W. Yang, "Minimizing LR(1) state machines is NP-hard,"
arXiv:2110.00776, 2021.

[6] D. Grune and C. J. H. Jacobs, *Parsing Techniques: A Practical Guide*,
2nd ed., Springer, 2008.

[7] J. Gallier, notes on LR parsing and the item-NFA construction,
University of Pennsylvania.

[8] E. W. Dijkstra, "ALGOL-60 translation," Stichting Mathematisch
Centrum, Rekenafdeling, ALGOL Bulletin Supplement nr. 10, 1961 (the
shunting-yard algorithm).

[9] Gazelle blog series, part 9: conflict counterexamples.

[10] D. R. Tribble, "The Honalee LR(k) algorithm,"
http://david.tribble.com/text/honalee.html.

[11] S. Heilbrunner, "A parsing automata approach to LR theory,"
*Theoretical Computer Science* 15, 1981.

[12] J. Zimmerman, "Practical LR parser generation," arXiv:2209.08383,
2022.

[13] S. Kannapinn, "Eine Rekonstruktion der LR-Theorie zur Elimination von
Redundanz mit Anwendung auf den Bau von ELR-Parsern," Dissertation,
Technische Universität Berlin, 2001.

[14] E. Scott and A. Johnstone, "Generalized bottom up parsers with
reduced stack activity," *The Computer Journal* 48(5), 2005.
