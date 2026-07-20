# The Parse Table Is Just an Automaton

*Minimal LR(1) tables from generic DFA algorithms — how one encoding decision
makes conflict resolution, table compression, and runtime precedence fall out
of subset construction and partition refinement.*

## Abstract

We present the construction of minimal LR(1) parse tables as implemented
in gazelle, our parser generator. The construction rests on one
representational decision: reduce actions are encoded as ordinary
labeled transitions into per-rule *reduce states* — equivalently, every
production is extended by its lookahead string and items walk the
extended production — so that the entire parse table, shifts, gotos, and
reduces alike, is the transition function of a single DFA. Under this
encoding the classical pipeline collapses into generic automaton
algorithms: canonical LR(1) construction is subset construction over an
NFA of items; a conflict is a reachable state rather than a corrupted
table cell, and conflict resolution is a single classifying pass over
states, run on the canonical automaton before any merging; table
compression is standard DFA minimization behind a 40-line
lookahead-alignment pass; operator precedence is deferred to parse time
by a symbol renaming that leaves every intermediate stage untouched. One
generic automaton module of under 300 lines builds both gazelle's lexer
and its parser.

We validate the construction against GNU Bison on five grammars (C++,
C11, Python, and gazelle's regex and meta grammars): the raw automaton
reproduces bison's canonical LR(1) state and conflict counts exactly,
and the minimized automaton reproduces bison's IELR(1) state counts
exactly — including the 30 states IELR splits beyond LALR on the C++
grammar. Because resolution precedes merging, the merge/resolution
interaction that IELR exists to repair cannot arise; because alignment
fixes the table's don't-care entries deterministically, minimization
computes a unique coarsest behavior-preserving merge in polynomial time,
sidestepping the search over completions whose general form is NP-hard.
The closest prior construction, Kannapinn's minimal-LR(k) machine,
carries reduce information as state output and explicitly rejects
minimizing the bare transition structure; the encoding presented here
removes exactly that obstacle.

## 1. Introduction

In theory, parsing was settled sixty years ago: LR is the most powerful
deterministic method — linear time, one token of lookahead, every
deterministic context-free language [1]. In practice, nearly every
production compiler ships a hand-written recursive-descent parser, and
anyone who has used the classic generators knows why: the extra build
step, semantic actions quoted inside a foreign grammar syntax, an API
that owns the main loop and calls you, and — above all — conflicts
reported in terms of the tool's internals, which under LALR may not even
be the grammar's fault.

Gazelle, our parser generator, was written against this list, on one
design principle: expose the algorithms rather than wrap them. The
pieces of LR parsing — construction, conflict analysis, table
compression — should be components a user can understand and compose
into the task at hand, not a framework that solves a predetermined task.
Such a philosophy stands or falls with comprehensibility, and this paper
is its test: LR parsing, presented as gazelle implements it.

The paper is one derivation followed by its dividends. §2 rebuilds the
LR parser from first principles: parsing as reinserting the parentheses
a parse tree elides; the parser as an item stack driven by an NFA;
LR(0) as that machine's direct determinization; and LR(1) as a loop
rotation that moves the reduce decision past one token. The derivation
lands on gazelle's one non-standard representational choice — reduce
actions are ordinary labeled transitions into per-rule reduce states,
equivalently items walk productions extended by their lookahead — which
makes the entire parse table the transition function of a single DFA.
§3 collects what that choice pays for:

- a conflict is a reachable state, and conflict resolution is a
  twenty-line classification pass, run on the canonical automaton
  before any merging (§3.3);
- table compression is generic DFA minimization behind a 40-line
  alignment pass, and reproduces IELR(1)'s state counts exactly (§3.4);
- operator precedence is a symbol renaming whose resolution is deferred
  to parse-time data on the token (§3.5);
- k tokens of lookahead is k more dot positions, with no other change
  anywhere in the pipeline (§3.4).

§4 validates these claims against GNU Bison on five grammars — exact
canonical LR(1) state and conflict counts, exact IELR(1) state
counts — and §5 places the construction against prior work, in
particular Kannapinn's dissertation, the closest anticipation we know
of. §6 records engineering notes; §7 concludes. One pair of numbers
summarizes the paper: the same 300-line automaton module builds
gazelle's lexer and its parser, and on a real C++ grammar its tables
shrink from 5350 canonical states to 601 — precisely bison's IELR
count.

## 2. The LR parser, from first principles

This section derives the LR parser as one continuous construction: a
task (reinsert the parentheses the parse tree elides), a machine (an
item stack driven by an NFA), and two determinizations — LR(0)
directly, then LR(1) after one loop rotation.

### 2.1 Parsing is reinserting parentheses

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

The parentheses are the point. A grammar does more than carve a set of
valid strings out of the set of all token streams: it assigns each
valid stream a structure, and the labels on the parentheses — `add`,
`num` — are where semantics attach; they name the tree's constructors.
Two grammars can accept exactly the same strings and parenthesize them
differently: same language, different interpretation. "Parsing a
language" is in this sense a loose phrase. One parses with a *grammar*,
because what the rest of the compiler consumes is the grammar's
interpretation of the input, not the fact of its validity — so a
parsing method is useful only if it accepts the grammar whose
parentheses you mean, not merely some grammar for the same strings.

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
doesn't know it yet. What deleting the parentheses took away is the answer
to exactly one question: *which production, here?* Call whatever answers
it the **oracle**. An LL parser must consult the oracle at every `(r`,
before the body — and answers it by peeking a token or two ahead:
dispatch on the peek is recursive descent, and every parser written by
hand is exactly this. An LR parser consults the oracle only at each
`)r`, the whole body in view — a strictly later deadline, with strictly
more evidence. LL buys program structure by committing at the earliest
possible moment; LR commits at the last possible moment, and pays by
needing a machine where LL needed only a function call. Building that
machine — and replacing its oracle with computation — is the subject of
§2.2.

### 2.2 The parse loop: an NFA driving a stack

The machine is built from one part. A **dotted production**, or *item* —
a `(rule, dot)` pair — marks a position inside a production: the item
`expr → expr • PLUS term` reads "parsing an `add`; the left operand is
finished; `PLUS` must come next." An item is the entire control state
of one parsing hypothesis, and it has three possible moves — the three
edge kinds of an NFA whose states are items:

```
ADVANCE   (r, dot) --X-->  (r, dot+1)   the dot steps over X = rhs(r)[dot]:
                                        a token, or a nonterminal delivered
                                        by a CLOSE below
DESCEND   (r, dot) --ε-->  (b, 0)       rhs(r)[dot] is a nonterminal: enter
                                        a guessed production of it — silent
CLOSE     (r, end)                      complete: emit ")r"; lhs(r) is
                                        delivered to the suspended item
                                        below, which ADVANCEs over it
```

A parse in progress is a stack of hypotheses — the **item stack** — top
item active, every item beneath suspended mid-production, plus a value
stack of finished trees. The fundamental parse loop applies one move
per iteration to the top of the item stack:

```rust
let mut items = vec![Item { rule: START, dot: 0 }];   // the item stack
let mut trees = Vec::new();                            // the value stack

loop {
    let Item { rule, dot } = *items.last().unwrap();
    if dot == rhs_len(rule) {
        // CLOSE — ")rule": pop the item, deliver lhs(rule) to the parent.
        let children = trees.split_off(trees.len() - rhs_len(rule));
        trees.push(Tree::Node(rule, children));
        items.pop();
        match items.last_mut() {
            Some(parent) => parent.dot += 1,     // the parent's ADVANCE, on lhs(rule)
            None => return trees.pop(),          // __start closed: the parse
        }
    } else {
        match rhs(rule)[dot] {
            // ADVANCE — the dot steps over the arriving token.
            Terminal(t) => {
                trees.push(Tree::Leaf(expect(t)));
                items.last_mut().unwrap().dot += 1;
            }
            // DESCEND — push a guessed production of b.
            NonTerminal(b) => items.push(Item { rule: choose(b), dot: 0 }),
        }
    }
}
```

In the world that survives `NUM PLUS NUM`, the run looks like this:

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

Two properties are visible in this form. **There is exactly one
decision**: whether CLOSE fires, and with which production. Everything
else is dictated — ADVANCE is forced by what arrives, and a DESCEND's
guess (`choose`, the machine's only nondeterminism) is *silent*: it
consumes nothing, emits nothing, and is not used until that item's own
CLOSE fires. **And only CLOSE reads the rule in earnest** — the arity
to pop, the label to emit. The oracle question of §2.1 now has an exact
address: it is the CLOSE branch, and nothing else.

Two reshapings put this machine into its final form. Both change
bookkeeping only; neither touches what is computed.

**First reshaping: drive the loop by the token.** Package the two
stacks as a state machine whose single operation is `push(t)`: per
token, a burst of input-free moves — DESCENDs and CLOSEs — then exactly
one consuming move:

```rust
impl Parser {                       // state: items, trees
    fn push(&mut self, t: Token) -> Result<Step, SyntaxError> {
        loop {
            let Item { rule, dot } = *self.items.last().unwrap();
            if dot == rhs_len(rule) {
                /* CLOSE — exactly as above; input-free.       */
                /* closing __start returns Step::Done(tree).   */
            } else {
                match rhs(rule)[dot] {
                    // DESCEND — input-free.
                    NonTerminal(b) => self.items.push(Item { rule: choose(b), dot: 0 }),
                    // ADVANCE — the one consuming move: shift t.
                    Terminal(k) if t.kind == k => {
                        self.trees.push(Tree::Leaf(t));
                        self.items.last_mut().unwrap().dot += 1;
                        return Ok(Step::More);
                    }
                    Terminal(_) => return Err(SyntaxError),
                }
            }
        }
    }
}
```

The caller owns the loop — the inversion of control complained about in
§1 is resolved by the formulation itself, and this function is
gazelle's shipped API (`parser.push(token)`) in miniature. Note what
`t` is during the burst: already read, not yet shifted — in hand. This
machine never peeks, here or later; a token that has arrived but not
committed is a pipe of length one between stream and stack. EOF is a
token like any other — the final closes fire in its presence, `__start`
pops, accept is nothing special. And a failed `push(t)` means exactly
that no surviving world can consume `t`: the earliest point at which
failure is knowable.

**Second reshaping: make every move a table lookup.** Change the stack
discipline: ADVANCE *pushes* its successor instead of mutating in
place — one item cell per committed symbol, exactly parallel to the
value stack — and DESCEND dissolves into the lookup itself: a
transition first follows ε-edges (silently, on a guess) to an item
whose dot faces the symbol, then advances over it. Every move is now
one primitive, `nfa_transition(state, symbol)`, where a symbol is a
terminal from the pipe or the nonterminal a close just produced:

```rust
// One symbol step, after silently guessed ε-descends: follow ε-edges to
// an item whose dot faces `sym`, then advance over it.
fn nfa_transition(state: ItemState, sym: Symbol) -> Option<ItemState>;

impl Parser {                       // items: one cell per committed symbol
    fn push(&mut self, t: Token) -> Result<Step, SyntaxError> {
        loop {
            let top = *self.items.last().unwrap();
            if let Some(rule) = completed(top) {
                // CLOSE — strip the body, then step over the produced symbol.
                let n = rhs_len(rule);
                let children = self.trees.split_off(self.trees.len() - n);
                self.trees.push(Tree::Node(rule, children));
                self.items.truncate(self.items.len() - n);
                if rule == START { return Ok(Step::Done(self.trees.pop().unwrap())); }
                match nfa_transition(*self.items.last().unwrap(), lhs(rule).into()) {
                    Some(next) => self.items.push(next),
                    None => return Err(SyntaxError),
                }
            } else {
                // SHIFT — the same step, on the pipe's token.
                match nfa_transition(top, t.into()) {
                    Some(next) => {
                        self.items.push(next);
                        self.trees.push(Tree::Leaf(t));
                        return Ok(Step::More);
                    }
                    None => return Err(SyntaxError),
                }
            }
        }
    }
}
```

CLOSE strips the production's body — `len(rule)` cells off both
stacks — and takes one ordinary transition on the produced symbol from
the newly exposed cell; SHIFT is the identical operation on the pipe's
token. Two branches, one primitive, and the item stack now mirrors the
value stack cell for cell.

**Why this machine determinizes.** Look at where the nondeterminism
ended up: entirely inside `nfa_transition`. A wrong guess is a state
path that dies — never a different stack motion. The NFA is hiding the
backtracking: no world ever rewinds the stack or the input; a world
fails by its state path going dead, and most steps change *state*, not
*control*. What is pushed, how many cells strip, when the token
commits — all of it is common across worlds, with a single exception:
the CLOSE branch, whether the top is complete and with which rule.
Every disagreement between worlds is funneled into that one visible
place. This is the property that makes the next move legal — a stack
machine cannot be determinized in general.

### 2.3 Determinize: the LR(0) automaton

So carry all the worlds at once: let each cell of the item stack hold
the *set* of items the worlds could occupy there. The set version of
`nfa_transition` — the union over members, ε-edges and all — is a plain
function from set to set: no guess survives it, and the reachable sets
are finitely many. This is the **subset construction**. (It is also Knuth's
foundational observation [1]: the stacks from which a parse can still
succeed — the *viable prefixes* — form a regular language, and a cell's
set is that language's state after the symbols beneath.) The cells
after committing `expr PLUS NUM`:

```
{ __start → •expr, expr → •expr PLUS term, expr → •term, term → •NUM, … }
   --expr-->  { __start → expr•, expr → expr •PLUS term }
   --PLUS-->  { expr → expr PLUS •term, term → •NUM, term → •LPAREN expr RPAREN }
   --NUM-->   { term → NUM• }
```

A close `)r` for `B → γ` is viable exactly when the top cell contains
the completed item `B → γ •`: the symbol steps that reached it put γ on
top of the stack, and the ε-edge that entered `B → • γ` came from a
suspended parent whose dot stands before `B` — the two conditions of a
correct close, established by membership in one set. Call a completed
item in the top cell a **verdict**. Here there is one: `)num`, the
unique viable move. Build the cells for the trap stack `expr PLUS expr`
instead and the third transition has no edge on `expr` — the set goes
empty; nothing is viable, which is exactly what "dead" means, detected
by a table lookup.

Name what has just been built. These sets are the classical **LR(0)
item sets**, and the set-to-set table is the LR(0) automaton: the
direct determinization of the bare item NFA, no lookahead anywhere in
the machine. Grammars for which it never faces a choice — every
reachable top cell holding either a single verdict and no viable shift,
or no verdict at all — are exactly the **LR(0)** grammars. The class is
real but cramped, and the run above already shows why: in its second
cell, `__start → expr•` is a verdict *and* `PLUS` is shiftable — an
LR(0) shift/reduce conflict. The expression grammar itself is out of
reach.

### 2.4 One token in the pipe: the rotation to LR(1)

The tie-breaker is the pipe's token, pushed *into* the NFA: annotate every
item with a **follow-token**, the token that must arrive after its
production. The epsilon edge from `A → α • B β` with follow-token `t`
spawns `B → • γ` with follow-tokens drawn from FIRST(β·t) — whatever can
actually follow this particular `B`. A completed item then delivers its
verdict only when the pipe holds its follow-token. Item plus
follow-token is the classical LR(1) item; grammars for which the refined
verdict list is a singleton everywhere — shift, or one specific reduce — are
exactly **LR(1)**: the oracle replaced by a table lookup and the one
token in the pipe.

Wiring them in is a *loop rotation* — and the rotation is the entire
distance from LR(0) to LR(1). Look at where §2.2's loop asks its
question: at the top of each iteration, on the pre-step state —
`completed(top)`, consulted before the pipe token is so much as looked
at. That is the LR(0) schedule: close immediately upon completion. As
an automaton move such an input-free close is an ε-edge, and subset
construction swallows ε-edges: the reduction survives only as an
annotation on the state — "this set contains `B → γ •`, so reducing
`B → γ` is on offer here." Every textbook parse table is exactly this
encoding, reduce actions as annotations, and annotations are invisible
to generic automaton algorithms.

Now rotate: move the dispatch from before the step to after it. Each
iteration takes one transition first — on the pipe's token, or on a
just-produced nonterminal — and branches on *where it lands*. Nothing
computed changes; what moves is the close decision, across the arrival
of exactly one token, so that the machine decides with evidence in
hand. This is what "one token of lookahead" *means*, operationally.
But the rotated dispatch needs a landing state to find — completion
must be somewhere a lettered step can arrive. So give every production
one extra NFA state, its **reduce node**, and let the completed item
step to it on the pipe's content — an ordinary lettered edge, labeled
by precisely the follow-tokens computed above:

```
B → γ •   --t-->   reduce node of B → γ
```

Said as compactly as possible: *append the follow-token to the
production, and keep walking the dot.* The LR(1) item — `B → γ •` with
follow-token `t` — is an ordinary dotted position in the extended
production `B → γ t`; the edge above is not a new kind of transition
but the same dot-advance as every other edge, stepping over the
appended symbol; and the reduce node is the dot falling off the
extended end — a *completed extended item*. One node per rule suffices:
the completed positions differ only in which follow-token was stepped
over, and after the close the machine needs only the rule — arity and
label. The machine has exactly one operation, advance the dot —
positions inside γ step over committed stack symbols, appended
positions step over pipe content. The appended position is also a free
slot, and it is where the lookahead filter installs. Extend every
production by a wildcard token and the machine is LR(0); by FOLLOW(B),
SLR(1); by the path-precise follow-token computed above, canonical
LR(1); by k tokens — a pipe of length k, one more dot per token —
LR(k), with no change anywhere else in the pipeline. The rotation
creates the slot; the filter fills it. Accept stops being special — it
is the reduce node of the augmented `__start → expr`.

The compiled parser is now the second reshaping's function with its two
open ends closed by the construction: `nfa_transition` becomes
`TRANSITION`, the subset machine's total, deterministic table — and the
close's trigger, `completed(top)`, becomes *the transition on the pipe
token landing in a reduce state* — the loop rotation, compiled. The
kind of state a step lands in *is* the event:

```rust
impl Parser {                       // states: one DFA state per committed symbol
    fn push(&mut self, t: Token) -> Result<Step, SyntaxError> {
        loop {
            match TRANSITION[*self.states.last().unwrap()][t.kind] {
                Target::Reduce(rule) => {
                    // CLOSE — ")rule", licensed by the pipe token.
                    let n = rhs_len(rule);
                    let children = self.trees.split_off(self.trees.len() - n);
                    self.trees.push(Tree::Node(rule, children));
                    self.states.truncate(self.states.len() - n);
                    if rule == START { return Ok(Step::Done(self.trees.pop().unwrap())); }
                    let next = TRANSITION[*self.states.last().unwrap()][lhs(rule)];
                    self.states.push(next.unwrap_item());   // goto: the same step, on lhs
                }
                Target::Item(next) => {
                    // SHIFT — commit the pipe token.
                    self.states.push(next);
                    self.trees.push(Tree::Leaf(t));
                    return Ok(Step::More);
                }
                Target::Error => return Err(SyntaxError),
            }
        }
    }
}
```

Line for line, this is the second reshaping's loop — same control flow,
same stack motions — with the state type widened from item to item set
and the guessing gone from the lookup. A close never touches the pipe;
the token stays in hand across an entire cascade — after the first
`NUM` of `NUM PLUS NUM`, the one token `PLUS` licenses `)num`, then
`)term`, before it finally commits — and each close's goto is an
ordinary transition on the produced nonterminal from the cell the strip
exposes. For an LR(1) grammar exactly one arm matches at every step —
the case where two could is §3.3.

The rotated schedule is also where canonical LR(1)'s *immediate error
detection* comes from. A machine that reduces on completion alone —
LR(0)'s schedule — happily performs closes that are already doomed and
discovers the error a few steps later; a reduce edge fires only when
the actual next token licenses it, so this machine never does provably
futile work. Hold that thought for §3.4, where table compression
deliberately sells fragments of that precision back.

And note what the cells are. Each one is the precise annotation of
*where the parse stands* after the symbols beneath it — the set of
positions the guessing machine could occupy, which is everything the
future can ever need to know about those symbols. The cell a close
exposes is the resumption point, the role a return address plays on a
call stack: the position the machine comes back to once everything
above it has closed — still valid, never recomputed, because a cell is
a function of the symbols below it and a close touches none of them.
The grammar symbols themselves are never stored — every decision reads
only the cells — which is why the textbook parser doesn't store them
either. The "stack of LR states" was never mysterious: it is this
section's first item stack, determinized cell by cell, and the parser
is a machine that keeps the annotation current.

Two boundary lines keep this section's claims honest. Knuth's
regularity is unconditional — it holds for every context-free grammar,
ambiguous ones included — but it is a statement about the *past* only:
everything the consumed input can contribute to any parsing decision
fits, lossless, in one DFA state. And it promises possibility, never
uniqueness. The top cell holds the exact set of viable verdicts;
whether that set collapses to one is a question about the *future* —
does evidence within one token settle it? — and LR(1)-ness is exactly
the property that it always does. Neither regularity nor unambiguity
implies it:

```
s = x list A => sa | y list B => sb;
x = C => x;
y = C => y;
list = D list => more | D => done;
```

After committing `C`, the top cell reports — with complete precision —
that either `x → C` or `y → C` ends here, and the token that decides,
`A` or `B`, sits beyond arbitrarily many `D`s. The grammar is
unambiguous; its language is even regular; no lookahead k rescues it.
The past is fully summarized — the future is simply out of reach.
(Knuth also proved the language-level consolation: every deterministic
language has *some* LR(1) grammar [1]. §2.1 says why it consoles less
than it seems: a rewritten grammar parenthesizes differently, and the
parentheses are what we came for.) And when even unbounded lookahead
would leave two whole parses of one input standing, the grammar is
ambiguous, and the residual choice is the subject of §3.3.

## 3. Gazelle

§2 ended with the complete parser: one transition function, one loop.
Two practical problems stand between that machine and a usable tool:
canonical tables are far too large to ship, and real grammars have
conflicts. This section presents how gazelle solves both with generic
automaton algorithms — beginning with how the field has solved them for
fifty years, and why that machinery is the opaque part of every
classical generator.

### 3.1 Merge early, repair later: LALR, Pager, and IELR

Knuth's canonical LR(1) construction [1] settled the theory in 1965 and
was impractical on arrival: the automaton distinguishes parser states by
their full one-token right context, and the bookkeeping multiplies
states. Our C++ grammar yields 5350 canonical states; a LALR table for
the same grammar has 571. For fifty years the standard response has been
to avoid building the canonical automaton at all and to construct a
smaller one directly, merging states that look like they behave alike.
Each generation of tools merges more carefully than the last, because
each discovered a new way merging goes wrong.

**LALR** [2] merges maximally: every pair of states with the same core —
the same items, ignoring lookaheads — becomes one state, and their
lookahead sets are unioned. This is yacc's and bison's default, and for
most grammars it works. Its first failure mode is the famous one:
merging can manufacture conflicts. Take the textbook grammar

```
s = A x A => axa | B x B => bxb | A y B => ayb | B y A => bya;
x = T => x;
y = T => y;
```

After `A T` the canonical automaton reduces `T` to `x` on lookahead `A`
and to `y` on lookahead `B`; after `B T`, exactly the reverse. Two
states, no conflict — the grammar is LR(1). But the two states share a
core, so LALR merges them, the lookahead sets union, and the merged
state can reduce `T` to either `x` or `y` on either lookahead: a
reduce/reduce conflict that exists in no canonical state. The user wrote
a correct grammar and is told it is broken, by a diagnostic
("reduce/reduce conflict in state 217") that names an artifact of the
merge. This is the inexplicable-conflict experience of §1, and it is not
user error; it is the tool's approximation leaking.

LALR's second failure mode is quieter and worse. When conflicts are
*intended* — an ambiguous expression grammar plus precedence
declarations — resolution runs on the merged automaton's table cells. A
lookahead that reached a cell from one context gets resolved by a
declaration meant for another; the parser still builds, no warning is
issued, and its behavior on valid inputs silently differs from what the
canonical automaton would do. Merging does not just misreport the
grammar; it can change the language the parser accepts.

**Pager's method** [3] repairs the first failure: merge two states only
when a *weak compatibility* test proves the union cannot create a
conflict. For LR(1) grammars this yields small conflict-free tables. But
the merging happens during construction, so the result depends on the
order in which states are generated — and the compatibility test asks
only whether a conflict would appear, nothing about how it would be
resolved. For the intentionally ambiguous grammars, the second failure
mode survives intact.

**IELR(1)** [4] confronts the second failure directly, and its
architecture is a diagnosis of the whole family. It builds the LALR
automaton first, then computes — by annotating lookaheads with their
provenance and propagating the annotations across the automaton —
exactly which merges changed an outcome of conflict resolution, and
splits those states back apart. It is correct: the resulting parser
behaves as the canonical one everywhere. It is the state of the art,
implemented in bison. But look at what it took: a five-phase pipeline in
which conflict resolution is the *final* phase, run after an
approximation (LALR), an analysis of the approximation's damage (the
annotations), and a repair (the splits). The machinery answers "what do
I compute next," never "what is this object" — comprehensible only
operationally.

The family has a common root. Merging interacts with conflict
resolution, and every tool in the family merges *before* resolving — so
each needs machinery to predict the interaction (Pager's compatibility
test) or to repair it (IELR's provenance analysis), and that machinery
is what makes the tools opaque. Even then none of it is optimal: finding
a minimal conflict-free merge of canonical states is NP-hard in general
[5]. Gazelle inverts the order. Build the canonical automaton, whose
behavior is right by construction; resolve conflicts there, where
resolution is trivially faithful; only then shrink, letting a generic
DFA minimizer merge exactly the states whose behavior came out
identical. No prediction, no repair — merging behaviorally identical
states cannot change behavior. The price of the inversion is that the
parse table must become the kind of object a DFA minimizer accepts: a
plain automaton — which is exactly the object §2 built. The rest of
this section runs the generic algorithms on it.

### 3.2 The encoding: reduce actions as transitions

The item half of §2's NFA is the textbook construction [6, 7]. The verdict
half — reduce nodes as ordinary states, reduce actions as ordinary edges —
is, as far as we can tell, absent from the literature (§5; Kannapinn [13]
comes closest: his machine carries reduce information as Moore-style
state output, which is the immediate-schedule encoding above, and he
explicitly dismisses minimizing the bare transition structure), and it
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
sinks — no outgoing edges. And the flat `(dot, la)` index is the
extended-production reading written in code: it enumerates exactly the
dotted positions of `rhs · la`.

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
§4: distinct item sets match `bison -Dlr.type=canonical-lr` state-for-state
on all five test grammars.

### 3.3 The residual oracle: conflicts are states

§2 left one loose end, deliberately. Control flow is common to every
world *except at CLOSE* — whether one fires, and for which rule — and
the loop rotation funneled every such disagreement into a single
visible place: the state a step lands in. For an LR(1) grammar the
landing state always speaks with one voice — an item state, or one
reduce node. For any other grammar, the residual oracle lives in
exactly the landing states that still carry two answers. What do those
look like?

The DFA is deterministic by construction — one transition per
(state, symbol). So a shift/reduce conflict cannot be a clash of edges; it
lands in the *target state*. If some item in state S shifts terminal `a`
and some completed item in S delivers its verdict on follow-token `a`, subset
construction builds a single transition on `a` to a **hybrid** target:
{advanced items} ∪ {reduce nodes} — a state on which both arms of §2.4's
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
identical behavior and the minimizer (§3.4) merges them. The only place the
duplicates are ever visible is in raw conflict counts, which can exceed
bison's per-(state, token) counts (§4).

Because a conflict is a state of a live automaton, generating a *witness*
for one — an input prefix reaching it, and completions showing the two runs
the machine cannot tell apart — is a graph search over δ rather than a
separate theory; that machinery is described elsewhere [9].

### 3.4 Small tables: align, then minimize

The gap to close is §3.1's: 5350 canonical item sets on the C++ grammar
against LALR's 571 (state counts here and in §4 discount bison's
synthetic `$accept` state). Two boundary markers frame the design
space: finding a minimal conflict-free merge of canonical states is
NP-hard in its general formulation [5], and the one post-hoc
construction in the literature, Kannapinn's [13], minimizes the
completed canonical machine by partition refinement but carries reduce
information as state annotations rather than transitions, and handles
only conflict-free grammars (§5).

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
tables; what §4 verifies is that the state counts coincide exactly,
including the hard case.

This also locates gazelle precisely relative to the NP-hardness of
optimal LR(1) merging [5]. That result concerns a search with
don't-cares: over all merges of canonical states that leave the table
conflict-free, where merging two states can enable or forbid merging
others — compatibility is not transitive, and graph coloring hides in
exactly that freedom. Gazelle never enters the search space. Alignment
fixes the don't-care entries once, deterministically — fill only where
every sibling agrees — and after that "mergeable" is behavioral
equivalence: transitive, with a unique coarsest solution that partition
refinement finds in polynomial time. What escapes the hardness is not
cleverness but the objective: we compute the exact minimum of a *fixed*
behavior, not the minimum over every sound completion. Nothing
certifies that a craftier choice of spurious entries could not merge
further on some grammar — that residual freedom is where the
NP-hardness lives — but the empirical answer of §4 is that the
fixed-behavior minimum already lands on IELR's counts.

The extended-production view of §2.4 also prices out LR(k). k tokens of
lookahead is k appended dot positions — the pipe's contents become part
of the state, the alphabet stays the same, and the pipeline runs
verbatim; only the NFA generator knows what k is. The canonical machine
grows by the pipe dimension, but the minimizer merges every pair of
states whose buffered tokens never influence a decision, so the shipped
table should land near LR(1)-minimal, splitting only where a second
token genuinely decides — the same pay-only-for-behavior mechanism that
lands LR(1) on the IELR counts. We have not run this experiment; the
point is that the encoding reduces it to one.

### 3.5 Runtime precedence: consulting the oracle at parse time

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
argument of §3.4 rests on an invariant — a state with no transition on a
symbol can never see that symbol on valid input, so filling the gap with a
spurious reduce is unobservable. Virtual twins break the invariant: a state
can lack the *virtual* reduce edge while shifting the *real* token, because
the canonical automaton shifts unconditionally there. Filling that gap does
not add an error-path reduction; it manufactures a `ShiftOrReduce` entry in
a state where canonical LR(1) has no question to defer, and the token's
precedence can then drive the parser off a valid input. The guard is one
condition: never fill a virtual reduce edge into a state that has a
transition on its real twin. It is not free — on the C11 grammar (with its
`prec` operators; the §4 comparison strips them, so no virtual symbols
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

## 4. Validation against Bison

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
exposes the hybrid-state artifact from §3.3: gazelle's raw count is 3182 S/R
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

## 5. Related work

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
locates the difficulty gazelle sidesteps (§3.4). Zimmerman's langcc [12]
represents the other contemporary full-LR(1) effort; it deliberately avoids
constructing the canonical automaton, attacking table size with grammar
transformations (CPS) and construction-time follow-set partitioning rather
than post-hoc minimization — the approaches compose rather than compete.

## 6. Engineering notes

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

## 7. Conclusion

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
