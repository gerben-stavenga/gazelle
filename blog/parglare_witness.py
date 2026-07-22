"""Witness: LALR merging defeats table-local dynamic disambiguation.

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
   unparseable because filtering cannot restore the missing reduction.
2. prefer_shifts=False: the merged cell defers to the filter — in both
   contexts. At the decision point, 'a x z' and 'b x z' present the same
   automaton state and the same remaining input ('z') but require opposite
   actions. Hence no filter that depends only on the state, candidate action,
   semantic subresults, and remaining input is correct for both; the two local
   policies below fail on complementary valid inputs.
3. A history-aware filter can parse all four by inspecting the consumed input.
   This is the deliberate oracle escape: arbitrary callback code can rebuild
   information that the table erased, but the callback has then assumed part
   of the parser's job. A shadow canonical parser would work for the same
   reason. The witness is a separation of table-local mechanisms, not a claim
   that no unrestricted Python program can compensate for merging.

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


def make_local_policy(reduce_on_single_z):
    def policy(context, from_state, to_state, action, production, subresults):
        if from_state is None:          # parglare's initialization probe call
            return True
        if not in_conflicted_cell(from_state):
            return True
        ahead = context.input_str[context.position:].strip()
        if not ahead.startswith("z"):
            return True
        # Both policies make the necessary shift when two z's remain. They
        # differ only on the indistinguishable local configurations a/x/z and
        # b/x/z, where exactly one z remains.
        should_reduce = reduce_on_single_z and ahead == "z"
        if action == REDUCE:
            return should_reduce
        if action == SHIFT:
            return not should_reduce
        return True
    return policy


def history_aware_policy(context, from_state, to_state, action, production,
                         subresults):
    """Recover the erased context by rereading the consumed input."""
    if from_state is None or not in_conflicted_cell(from_state):
        return True
    ahead = context.input_str[context.position:].strip()
    if not ahead.startswith("z"):
        return True
    should_reduce = (context.input_str.lstrip().startswith("b")
                     and ahead == "z")
    if action == REDUCE:
        return should_reduce
    if action == SHIFT:
        return not should_reduce
    return True


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
    run("default settings: filtering cannot restore erased reduction",
        dynamic_filter=lambda *a: True)
    with contextlib.redirect_stdout(io.StringIO()):
        pass
    b = io.StringIO()
    with contextlib.redirect_stdout(b):
        parsers = [
            ("prefer_shifts=False, local SHIFT answer", Parser(
                GRAMMAR_OBJ, prefer_shifts=False,
                dynamic_filter=make_local_policy(False))),
            ("prefer_shifts=False, local REDUCE answer", Parser(
                GRAMMAR_OBJ, prefer_shifts=False,
                dynamic_filter=make_local_policy(True))),
            ("prefer_shifts=False, history-aware oracle", Parser(
                GRAMMAR_OBJ, prefer_shifts=False,
                dynamic_filter=history_aware_policy)),
        ]
    for label, parser in parsers:
        print(f"--- {label} ---")
        for s in INPUTS:
            try:
                parser.parse(s)
                print(f"  parse({s!r:10}) OK")
            except Exception as e:
                print(f"  parse({s!r:10}) FAILED ({type(e).__name__})")
    print()
    print("Expected: the local policies fail 'b x z' and 'a x z',")
    print("respectively; the history-aware policy parses every input.")
