"""Witness: LALR merging makes parglare's dynamic disambiguation unable to
express a context-dependent deferred choice that canonical LR(1) separates.

Grammar (unambiguous):
    S: 'a' M | 'b' M Z;
    M: E {dynamic} | E Z {dynamic};
    E: 'x';
    terminals Z: 'z' {dynamic};

Canonical LR(1): the state after 'a' E has items {M->E. {STOP}, M->E.Z} —
on 'z' it shifts unconditionally, no conflict. The state after 'b' E has the
same LR(0) core but M->E. carries lookahead {Z}: a genuine shift/reduce
question on 'z' ('b x z' must reduce, 'b x z z' must shift). parglare's
(modified-LALR) construction merges the two states, unioning lookaheads,
so ONE cell must answer for both contexts.

Consequences demonstrated below (parglare 0.21.1):
1. Default settings (prefer_shifts=True): the conflict is silently resolved
   to shift at construction despite the `dynamic` marks; valid 'b x z' is
   unparseable under every filter.
2. prefer_shifts=False: the merged cell defers to the filter — in both
   contexts. At the decision point, 'a x z' and 'b x z' present the same
   automaton state and the same remaining input ('z') but require opposite
   actions. Hence no filter that depends on the state, the candidate
   action, and any amount of remaining input is correct for both; the
   policies below fail on complementary valid inputs. Recovering the answer
   would require excavating the stack for left context that the merge
   erased — exactly the information a canonical-faithful table retains.

Gazelle's construction keeps the two contexts in separate states: the
'a'-context cell is a plain shift, the 'b'-context cell is the one deferred
entry, and the completion guard preserves that separation through
minimization (resolve-then-minimize.md §5).
"""
from parglare import Grammar, Parser
from parglare.parser import SHIFT, REDUCE

GRAMMAR = r"""
S: 'a' M | 'b' M Z;
M: E {dynamic} | E Z {dynamic};
E: 'x';

terminals
Z: 'z' {dynamic};
"""

INPUTS = ["a x", "a x z", "b x z", "b x z z"]


def in_conflicted_cell(from_state):
    kernels = {str(i).split("   ")[0] for i in from_state.kernel_items}
    return "3: M = E ." in kernels and "4: M = E . Z" in kernels


def make_policy(reduce_on_z):
    def policy(context, from_state, to_state, action, production, subresults):
        if from_state is None:          # parglare's initialization probe call
            return True
        if not in_conflicted_cell(from_state):
            return True
        ahead = context.input_str[context.position:].lstrip()
        if not ahead.startswith("z"):
            return True
        if action == REDUCE:
            return reduce_on_z
        if action == SHIFT:
            return not reduce_on_z
        return True
    return policy


def run(label, **kwargs):
    print(f"--- {label} ---")
    try:
        p = Parser(GRAMMAR_OBJ, **kwargs)
    except Exception as e:
        print(f"  construction failed: {type(e).__name__}")
        return
    for s in INPUTS:
        try:
            p.parse(s)
            print(f"  parse({s!r:10}) OK")
        except Exception as e:
            print(f"  parse({s!r:10}) FAILED ({type(e).__name__})")


if __name__ == "__main__":
    import io, contextlib
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        GRAMMAR_OBJ = Grammar.from_string(GRAMMAR)
        # swallow parglare's table dump on conflict reporting
        Parser(GRAMMAR_OBJ, prefer_shifts=False, dynamic_filter=lambda *a: True)
    run("default settings (prefer_shifts=True), any filter: question erased",
        dynamic_filter=lambda *a: True)
    with contextlib.redirect_stdout(io.StringIO()):
        pass
    import sys
    b = io.StringIO()
    with contextlib.redirect_stdout(b):
        pA = Parser(GRAMMAR_OBJ, prefer_shifts=False,
                    dynamic_filter=make_policy(False))
        pB = Parser(GRAMMAR_OBJ, prefer_shifts=False,
                    dynamic_filter=make_policy(True))
    print("--- prefer_shifts=False, SHIFT-on-z policy (context a's answer) ---")
    for s in INPUTS:
        try:
            pA.parse(s)
            print(f"  parse({s!r:10}) OK")
        except Exception as e:
            print(f"  parse({s!r:10}) FAILED ({type(e).__name__})")
    print("--- prefer_shifts=False, REDUCE-on-z policy (context b's answer) ---")
    for s in INPUTS:
        try:
            pB.parse(s)
            print(f"  parse({s!r:10}) OK")
        except Exception as e:
            print(f"  parse({s!r:10}) FAILED ({type(e).__name__})")
    print()
    print("Expected: each policy fails exactly the input the other context owns")
    print("('b x z' under SHIFT policy, 'a x z' under REDUCE policy).")
