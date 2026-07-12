//! Conflict detection, grouping, example generation, and ambiguity analysis.
//!
//! This module handles the full pipeline from raw LR conflicts to human-readable
//! conflict messages with example strings and ambiguous nonterminal identification.

use alloc::string::{String, ToString};
use alloc::{format, vec, vec::Vec};

use super::{DfaLrInfo, GrammarInternal, Item, LrNfaInfo};
use crate::automaton::Dfa;
use crate::grammar::SymbolId;

/// Memo key for a conflict example: (prefix, terminal, reduce rule, second reduce rule).
/// Conflicts sharing this tuple produce the same example.
type ExampleKey = (Vec<u32>, u32, usize, Option<usize>);

// ============================================================================
// Conflict detection
// ============================================================================

pub(crate) enum ConflictKind {
    ShiftReduce(usize),
    ReduceReduce(usize, usize),
}

/// Detect conflicts in the DFA without resolving them.
/// Returns conflict info with raw DFA state indices.
pub(crate) fn detect_conflicts(
    dfa: &Dfa,
    lr: &DfaLrInfo,
    nfa_info: &LrNfaInfo,
    grammar: &GrammarInternal,
) -> Vec<(usize, SymbolId, ConflictKind)> {
    let num_terminals = grammar.symbols.num_terminals();
    let mut conflicts = Vec::new();

    for source in 0..dfa.num_states() {
        if !lr.has_items(source) {
            continue;
        }
        for &(sym, target) in &dfa.transitions[source] {
            let virtual_terminal = nfa_info.reduce_to_real.get(&sym).copied();
            let terminal = if sym < num_terminals {
                sym
            } else if let Some(real) = virtual_terminal {
                real
            } else {
                continue;
            };

            // A virtual terminal represents only the reduce side of a modified
            // terminal's S/R conflict. Its real-terminal transition represents
            // the shift side, so the split is intentional and not itself a
            // reportable conflict.
            if virtual_terminal.is_none()
                && lr.has_items(target)
                && !lr.reduce_rules[target].is_empty()
            {
                for &rule in &lr.reduce_rules[target] {
                    conflicts.push((source, SymbolId(terminal), ConflictKind::ShiftReduce(rule)));
                }
            }

            // Multiple reductions on a virtual terminal are still a genuine
            // R/R ambiguity. Report it against the corresponding real terminal.
            if lr.reduce_rules[target].len() > 1 {
                let rules = &lr.reduce_rules[target];
                for i in 1..rules.len() {
                    conflicts.push((
                        source,
                        SymbolId(terminal),
                        ConflictKind::ReduceReduce(rules[0], rules[i]),
                    ));
                }
            }
        }
    }

    conflicts
}

// ============================================================================
// Conflict resolution
// ============================================================================

/// Resolved DFA state: either a reduce state (single rule) or an item state.
#[derive(Clone)]
pub(crate) enum DfaStateKind {
    Reduce(usize),
    /// Items as (rule, dot) pairs.
    Items(Vec<(usize, usize)>),
}

/// Resolve conflicts and classify each DFA state:
/// - SR (mixed states with items + reduces): shift wins → Items
/// - RR (multiple reduces): lower rule wins → Reduce(winner)
/// - Pure reduce → Reduce(rule)
/// - Pure items → Items(nfa_items)
pub(crate) fn resolve_conflicts(lr: DfaLrInfo, nfa_info: &LrNfaInfo) -> Vec<DfaStateKind> {
    lr.reduce_rules
        .into_iter()
        .zip(lr.nfa_items)
        .map(|(mut reduces, nfa_items)| {
            if !nfa_items.is_empty() {
                // SR: shift wins
                let items = nfa_items
                    .iter()
                    .map(|&idx| (nfa_info.items[idx].rule, nfa_info.items[idx].dot))
                    .collect();
                DfaStateKind::Items(items)
            } else if !reduces.is_empty() {
                // Pure reduce or RR: keep lowest-numbered rule
                reduces.sort();
                DfaStateKind::Reduce(reduces[0])
            } else {
                // Dead state (shouldn't normally happen)
                DfaStateKind::Items(Vec::new())
            }
        })
        .collect()
}

// ============================================================================
// Parser simulation
// ============================================================================

/// A parser configuration: current state + stack of previous states.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ParserConfig {
    state: usize,
    stack: Vec<usize>,
}

/// Helper functions for simulating an LR parser on the raw DFA.
struct ParserSim<'a> {
    dfa: &'a Dfa,
    lr: &'a DfaLrInfo,
    nfa_info: &'a LrNfaInfo,
    grammar: &'a GrammarInternal,
    num_terminals: u32,
}

impl<'a> ParserSim<'a> {
    fn new(
        dfa: &'a Dfa,
        lr: &'a DfaLrInfo,
        nfa_info: &'a LrNfaInfo,
        grammar: &'a GrammarInternal,
    ) -> Self {
        Self {
            dfa,
            lr,
            nfa_info,
            grammar,
            num_terminals: grammar.symbols.num_terminals(),
        }
    }

    /// Shift a terminal: transition to an item state.
    fn shift(&self, state: usize, terminal: u32) -> Option<usize> {
        self.dfa.transitions[state]
            .iter()
            .find(|&&(sym, target)| sym == terminal && self.lr.has_items(target))
            .map(|&(_, target)| target)
    }

    /// Goto on a non-terminal after a reduce.
    fn goto(&self, state: usize, nonterminal: u32) -> Option<usize> {
        self.dfa.transitions[state]
            .iter()
            .find(|&&(sym, target)| sym == nonterminal && self.lr.has_items(target))
            .map(|&(_, target)| target)
    }

    /// Check if a state can accept (reduce on EOF).
    fn can_accept(&self, state: usize) -> bool {
        self.dfa.transitions[state].iter().any(|&(sym, target)| {
            sym == 0 && !self.lr.has_items(target) && !self.lr.reduce_rules[target].is_empty()
        })
    }

    /// Get all reduce rules available on a given lookahead terminal.
    /// Checks both the real terminal and its virtual reduce symbol.
    fn reduces_on(&self, state: usize, terminal: u32) -> Vec<usize> {
        let syms = [Some(terminal), self.virtual_for(terminal)];
        self.dfa.transitions[state]
            .iter()
            .filter(|&&(sym, target)| syms.contains(&Some(sym)) && !self.lr.has_items(target))
            .flat_map(|&(_, target)| self.lr.reduce_rules[target].iter().copied())
            .collect()
    }

    /// Get the virtual reduce symbol for a terminal, if any.
    fn virtual_for(&self, terminal: u32) -> Option<u32> {
        self.nfa_info.real_to_virtual.get(&terminal).copied()
    }

    /// Get all symbols (terminals and non-terminals) that can be shifted from this state.
    fn shiftable_symbols(&self, state: usize) -> Vec<u32> {
        self.dfa.transitions[state]
            .iter()
            .filter(|&&(sym, target)| {
                sym != 0
                    && !self.nfa_info.reduce_to_real.contains_key(&sym)
                    && self.lr.has_items(target)
            })
            .map(|&(sym, _)| sym)
            .collect()
    }

    /// Apply a reduce to a parser config. Returns None if stack is too short or goto fails.
    fn apply_reduce(&self, cfg: &ParserConfig, rule_idx: usize) -> Option<ParserConfig> {
        let rule = &self.grammar.rules[rule_idx];
        let rhs_len = rule.rhs.len();
        let lhs_id = rule.lhs.id().0;

        let mut full = cfg.stack.clone();
        full.push(cfg.state);

        if full.len() <= rhs_len {
            return None;
        }

        full.truncate(full.len() - rhs_len);
        let goto_from = *full.last().unwrap();
        let goto_target = self.goto(goto_from, lhs_id)?;
        full.push(goto_target);

        let state = full.pop().unwrap();
        Some(ParserConfig { state, stack: full })
    }

    /// Shift a terminal in a config, returning the new config.
    fn shift_config(&self, cfg: &ParserConfig, terminal: u32) -> Option<ParserConfig> {
        let target = self.shift(cfg.state, terminal)?;
        let mut new_stack = cfg.stack.clone();
        new_stack.push(cfg.state);
        Some(ParserConfig {
            state: target,
            stack: new_stack,
        })
    }
}

// ============================================================================
// CST-based parser config (for ambiguity detection)
// ============================================================================

/// A CST node built during parser simulation.
#[derive(Clone, Debug)]
enum CstNode {
    /// A shifted symbol (terminal from BFS, or placeholder for prefix).
    Leaf { symbol: u32, is_conflict: bool },
    /// A reduced production. `is_conflict` marks the initial diverging reduction.
    Interior {
        rule: usize,
        children: Vec<CstNode>,
        is_conflict: bool,
    },
}

impl CstNode {
    fn set_conflict(&mut self) {
        match self {
            CstNode::Interior { is_conflict, .. } | CstNode::Leaf { is_conflict, .. } => {
                *is_conflict = true;
            }
        }
    }

    /// Count the number of leaves (terminals) in this CST — proxy for input span width.
    fn leaf_count(&self) -> usize {
        match self {
            CstNode::Leaf { .. } => 1,
            CstNode::Interior { children, .. } => children.iter().map(|c| c.leaf_count()).sum(),
        }
    }

    /// Check if this Interior node or any descendant has `is_conflict` set.
    /// Leaf conflicts don't count at the top level — they need to be
    /// consumed into a parent reduction first.
    fn has_conflict(&self) -> bool {
        match self {
            CstNode::Leaf { .. } => false,
            CstNode::Interior {
                is_conflict,
                children,
                ..
            } => *is_conflict || children.iter().any(|c| c.has_conflict_inner()),
        }
    }

    fn has_conflict_inner(&self) -> bool {
        match self {
            CstNode::Leaf { is_conflict, .. } => *is_conflict,
            CstNode::Interior {
                is_conflict,
                children,
                ..
            } => *is_conflict || children.iter().any(|c| c.has_conflict_inner()),
        }
    }
}

/// A parser config augmented with a CST on the stack.
/// `entries` is parallel to `config.stack`, `top` is the node for `config.state`.
#[derive(Clone)]
struct TrackedConfig {
    config: ParserConfig,
    entries: Vec<CstNode>,
    top: CstNode,
}

impl TrackedConfig {
    fn initial() -> Self {
        TrackedConfig {
            config: ParserConfig {
                state: 0,
                stack: vec![],
            },
            entries: vec![],
            top: CstNode::Leaf {
                symbol: 0,
                is_conflict: false,
            }, // phantom for initial state
        }
    }

    fn shift(&self, sim: &ParserSim, terminal: u32) -> Option<TrackedConfig> {
        let shifted = sim.shift_config(&self.config, terminal)?;
        let mut new_entries = self.entries.clone();
        new_entries.push(self.top.clone());
        Some(TrackedConfig {
            config: shifted,
            entries: new_entries,
            top: CstNode::Leaf {
                symbol: terminal,
                is_conflict: false,
            },
        })
    }

    fn reduce(&self, sim: &ParserSim, rule_idx: usize) -> Option<TrackedConfig> {
        let reduced = sim.apply_reduce(&self.config, rule_idx)?;
        let rhs_len = sim.grammar.rules[rule_idx].rhs.len();

        let mut full = self.entries.clone();
        full.push(self.top.clone());
        let children = full.split_off(full.len() - rhs_len);

        Some(TrackedConfig {
            config: reduced,
            entries: full,
            top: CstNode::Interior {
                rule: rule_idx,
                children,
                is_conflict: false,
            },
        })
    }
}

// ============================================================================
// Counterexample search
// ============================================================================

/// BFS budget: maximum number of config pairs generated before giving up.
const BFS_BUDGET: usize = 1_000;

/// Result of counterexample search for a single conflict.
enum Counterexample {
    /// Both parses converge: ambiguous nonterminal with two CST subtrees.
    Unifying(Convergence),
    /// Two strings sharing prefix + terminal but diverging after.
    NonUnifying {
        suffix_a: Vec<u32>,
        suffix_b: Vec<u32>,
    },
}

/// Build a TrackedConfig by replaying a prefix of symbols from state 0.
fn replay_prefix_tracked(sim: &ParserSim, prefix: &[u32]) -> Option<TrackedConfig> {
    let mut tc = TrackedConfig::initial();
    for &sym in prefix {
        tc = tc.shift(sim, sym)?;
    }
    Some(tc)
}

/// Advance a tracked config on lookahead terminal `t`: apply reduces triggered
/// by `t`, then shift `t`. Returns all reachable tracked configs after the shift.
fn advance_tracked(sim: &ParserSim, tc: &TrackedConfig, terminal: u32) -> Vec<TrackedConfig> {
    reduce_closure(sim, tc, terminal)
        .iter()
        .filter_map(|tc| tc.shift(sim, terminal))
        .collect()
}

/// Compute the reduce closure: all configs reachable from `tc` by pure reductions
/// triggered by lookahead `terminal`. Includes the original config.
fn reduce_closure(sim: &ParserSim, tc: &TrackedConfig, terminal: u32) -> Vec<TrackedConfig> {
    let mut results = vec![tc.clone()];
    let mut visited = alloc::collections::BTreeSet::new();
    visited.insert(tc.config.clone());
    reduce_closure_dfs(sim, tc, terminal, &mut results, &mut visited);
    results
}

fn reduce_closure_dfs(
    sim: &ParserSim,
    tc: &TrackedConfig,
    terminal: u32,
    results: &mut Vec<TrackedConfig>,
    visited: &mut alloc::collections::BTreeSet<ParserConfig>,
) {
    for rule in sim.reduces_on(tc.config.state, terminal) {
        if let Some(reduced) = tc.reduce(sim, rule)
            && visited.insert(reduced.config.clone())
        {
            results.push(reduced.clone());
            reduce_closure_dfs(sim, &reduced, terminal, results, visited);
        }
    }
}

/// Collect all symbols that could advance a config (direct shifts + terminals
/// that trigger reduces leading to shifts). Non-terminals sorted first.
fn candidate_symbols(sim: &ParserSim, state: usize) -> Vec<u32> {
    let mut syms = sim.shiftable_symbols(state);
    for t in 0..sim.num_terminals {
        if !sim.reduces_on(state, t).is_empty() {
            syms.push(t);
        }
    }
    syms.sort();
    syms.dedup();
    syms.sort_by_key(|&sym| if sym >= sim.num_terminals { 0 } else { 1 });
    syms
}

/// Extract the ambiguous nonterminal at convergence.
/// At convergence the tops differ (possibly same rule, different subtrees).
fn find_convergence(a: &TrackedConfig, b: &TrackedConfig, sim: &ParserSim) -> Option<Convergence> {
    if let CstNode::Interior { rule, .. } = &a.top {
        let lhs = sim.grammar.rules[*rule].lhs.id().0;
        return Some(Convergence {
            nonterminal: lhs,
            cst_a: a.top.clone(),
            cst_b: b.top.clone(),
        });
    }
    None
}

/// Find a common suffix that drives both parser configs to convergence.
///
/// Two-phase BFS: for each candidate symbol, first compute the reduce closure
/// on both sides (checking for convergence there), then shift to produce
/// the next BFS level. Uses TrackedConfig to identify the ambiguous
/// nonterminal when convergence is found.
/// Result of joint suffix search: the ambiguous nonterminal and two CST subtrees.
struct Convergence {
    nonterminal: u32,
    cst_a: CstNode,
    cst_b: CstNode,
}

fn find_joint_suffix(
    sim: &ParserSim,
    tc_a: &TrackedConfig,
    tc_b: &TrackedConfig,
) -> Option<Convergence> {
    use alloc::collections::{BTreeSet, VecDeque};

    if tc_a.config == tc_b.config {
        return find_convergence(tc_a, tc_b, sim);
    }

    let mut queue = VecDeque::new();
    let mut visited = BTreeSet::new();

    visited.insert((tc_a.config.clone(), tc_b.config.clone()));
    queue.push_back((tc_a.clone(), tc_b.clone()));

    while let Some((cur_a, cur_b)) = queue.pop_front() {
        let cands_a = candidate_symbols(sim, cur_a.config.state);
        let cands_b_set: BTreeSet<u32> = candidate_symbols(sim, cur_b.config.state)
            .into_iter()
            .collect();

        for t in cands_a {
            if !cands_b_set.contains(&t) {
                continue;
            }

            // Reduce closure — all configs reachable by pure reductions
            let closures_a = reduce_closure(sim, &cur_a, t);
            let closures_b = reduce_closure(sim, &cur_b, t);

            // Check convergence among reduced configs — prefer widest span
            // (deepest reduction) so that is_conflict is in the CST top.
            let mut best: Option<Convergence> = None;
            for ra in &closures_a {
                for rb in &closures_b {
                    if ra.config == rb.config {
                        if let Some(conv) = find_convergence(ra, rb, sim) {
                            let span = conv.cst_a.leaf_count() + conv.cst_b.leaf_count();
                            if best
                                .as_ref()
                                .is_none_or(|b| span > b.cst_a.leaf_count() + b.cst_b.leaf_count())
                            {
                                best = Some(conv);
                            }
                        }
                    }
                }
            }
            // Only return if is_conflict is visible in the CST tops.
            // Otherwise keep searching — the conflict node may be a sibling
            // that only becomes visible at a deeper convergence level.
            if let Some(ref b) = best {
                if b.cst_a.has_conflict() && b.cst_b.has_conflict() {
                    return best;
                }
            }

            // Shift to produce next BFS level
            let shifted_as: Vec<TrackedConfig> = closures_a
                .iter()
                .filter_map(|tc| tc.shift(sim, t))
                .collect();
            let shifted_bs: Vec<TrackedConfig> = closures_b
                .iter()
                .filter_map(|tc| tc.shift(sim, t))
                .collect();

            for sa in &shifted_as {
                for sb in &shifted_bs {
                    if visited.len() >= BFS_BUDGET {
                        return None;
                    }
                    if !visited.insert((sa.config.clone(), sb.config.clone())) {
                        continue;
                    }
                    queue.push_back((sa.clone(), sb.clone()));
                }
            }
        }
    }

    None
}

/// Spawn depth of each nonterminal wanted in `state`: 0 if wanted by a kernel
/// item (dot > 0), else one more than the depth of the closest LHS whose
/// closure spawned it. From any goto target of `state`, completing the item
/// with the minimal-depth LHS climbs strictly back toward a kernel item.
fn spawn_depths(sim: &ParserSim, state: usize) -> alloc::collections::BTreeMap<u32, usize> {
    use alloc::collections::btree_map::Entry;
    use alloc::collections::{BTreeMap, VecDeque};

    let items: Vec<Item> = sim.lr.nfa_items[state]
        .iter()
        .map(|&idx| sim.nfa_info.items[idx])
        .collect();
    let wants = |item: Item| -> Option<u32> {
        sim.grammar.rules[item.rule]
            .rhs
            .get(item.dot)
            .filter(|sym| sym.is_non_terminal())
            .map(|sym| sym.id().0)
    };

    let mut depths = BTreeMap::new();
    let mut queue = VecDeque::new();
    for nt in items
        .iter()
        .filter(|it| it.dot > 0)
        .filter_map(|&it| wants(it))
    {
        if depths.insert(nt, 0usize).is_none() {
            queue.push_back(nt);
        }
    }
    while let Some(lhs) = queue.pop_front() {
        let depth = depths[&lhs] + 1;
        for &item in items.iter().filter(|it| it.dot == 0) {
            if sim.grammar.rules[item.rule].lhs.id().0 != lhs {
                continue;
            }
            if let Some(nt) = wants(item)
                && let Entry::Vacant(e) = depths.entry(nt)
            {
                e.insert(depth);
                queue.push_back(nt);
            }
        }
    }
    depths
}

/// Find a suffix that drives a config to acceptance by completing productions on the stack.
///
/// Each round picks a kernel item of the current state, emits its remaining RHS
/// symbols (nonterminals shift as single symbols, so nothing is ever derived),
/// and reduces it. This terminates: reducing a dot >= 2 item strictly shrinks
/// the stack, and when only dot == 1 items exist the goto lands back over the
/// same base state, where picking the minimal spawn-depth LHS strictly climbs
/// toward a kernel item of the base — which then appears with dot >= 2.
fn find_independent_suffix(sim: &ParserSim, cfg: &ParserConfig) -> Vec<u32> {
    let mut suffix = Vec::new();
    let mut current = cfg.clone();

    loop {
        if sim.can_accept(current.state) {
            return suffix;
        }

        let (rule, dot) = {
            let items = || {
                sim.lr.nfa_items[current.state]
                    .iter()
                    .map(|&idx| sim.nfa_info.items[idx])
            };
            let remaining = |it: Item| sim.grammar.rules[it.rule].rhs.len() - it.dot;
            let chosen = items()
                .filter(|it| it.dot >= 2)
                .min_by_key(|&it| remaining(it))
                .or_else(|| {
                    let depths = spawn_depths(sim, *current.stack.last()?);
                    items().filter(|it| it.dot == 1).min_by_key(|it| {
                        let lhs = sim.grammar.rules[it.rule].lhs.id().0;
                        depths.get(&lhs).copied().unwrap_or(usize::MAX)
                    })
                })
                // Only reachable from state 0 (empty stack): all items have dot 0.
                .or_else(|| items().min_by_key(|&it| remaining(it)))
                .unwrap();
            (chosen.rule, chosen.dot)
        };

        for sym in &sim.grammar.rules[rule].rhs[dot..] {
            let sym_id = sym.id().0;
            suffix.push(sym_id);
            let target = sim.dfa.transitions[current.state]
                .iter()
                .find(|&&(s, t)| s == sym_id && sim.lr.has_items(t))
                .map(|&(_, t)| t)
                .unwrap();
            current.stack.push(current.state);
            current.state = target;
        }
        current = sim.apply_reduce(&current, rule).unwrap();
    }
}

/// Find a counterexample for a conflict.
///
/// Input: conflict state, lookahead terminal, first reduce rule, optional second
/// reduce rule (Some for R/R, None for S/R where shift is implicit).
///
/// Returns a `Counterexample` with either a unifying suffix (both parses converge,
/// with ambiguous NT info) or two independent suffixes.
fn find_counterexample(
    sim: &ParserSim,
    prefix: &[u32],
    terminal: u32,
    reduce_rule: usize,
    reduce_rule2: Option<usize>,
) -> Option<Counterexample> {
    let base = replay_prefix_tracked(sim, prefix)?;

    let (tc_a, tc_b) = if let Some(rule2) = reduce_rule2 {
        // R/R: both reduce from same base (pre-terminal configs)
        let mut tc_a = base.reduce(sim, reduce_rule)?;
        let mut tc_b = base.reduce(sim, rule2)?;
        tc_a.top.set_conflict();
        tc_b.top.set_conflict();
        (tc_a, tc_b)
    } else {
        // S/R: one shifts, other reduces then advances past terminal
        let mut tc_shift = base.shift(sim, terminal)?;
        tc_shift.top.set_conflict();
        let mut tc_reduced = base.reduce(sim, reduce_rule)?;
        tc_reduced.top.set_conflict();
        let tc_reduce_parse = advance_tracked(sim, &tc_reduced, terminal)
            .into_iter()
            .next()?;
        (tc_shift, tc_reduce_parse)
    };

    if let Some(conv) = find_joint_suffix(sim, &tc_a, &tc_b) {
        return Some(Counterexample::Unifying(conv));
    }

    // Compute full sentence for each path (including terminal + completion).
    // suffix_a is from shift side (terminal already consumed by tc_a).
    // suffix_b is from reduce side (terminal already consumed by tc_b).
    // Prepend terminal to make full post-prefix sentences.
    let mut suffix_a = vec![terminal];
    suffix_a.extend(find_independent_suffix(sim, &tc_a.config));
    let mut suffix_b = vec![terminal];
    suffix_b.extend(find_independent_suffix(sim, &tc_b.config));
    Some(Counterexample::NonUnifying { suffix_a, suffix_b })
}

// ============================================================================
// Example generation and conflict grouping
// ============================================================================

/// Generate example input strings that demonstrate each conflict.
///
/// Works on the raw DFA before Hopcroft minimization. Conflicts are grouped:
/// - S/R by (source_state, terminal) → multiple reduce rules per group
/// - R/R by (rule1, rule2) → multiple terminals per group
pub(crate) fn conflict_examples(
    dfa: &Dfa,
    lr: &DfaLrInfo,
    nfa_info: &LrNfaInfo,
    grammar: &GrammarInternal,
    conflicts: Vec<(usize, SymbolId, ConflictKind)>,
) -> Vec<crate::table::Conflict> {
    // BFS from state 0 to find shortest path (grammar symbols) to each state.
    let mut parent: Vec<Option<(usize, u32)>> = vec![None; dfa.num_states()];
    let mut visited = vec![false; dfa.num_states()];
    let mut queue = alloc::collections::VecDeque::new();
    queue.push_back(0usize);
    visited[0] = true;

    while let Some(state) = queue.pop_front() {
        if !lr.has_items(state) {
            continue;
        }
        for &(sym, target) in &dfa.transitions[state] {
            if nfa_info.reduce_to_real.contains_key(&sym) {
                continue;
            }
            if !lr.has_items(target) {
                continue;
            }
            if visited[target] {
                continue;
            }
            visited[target] = true;
            parent[target] = Some((state, sym));
            queue.push_back(target);
        }
    }

    let path_to = |target: usize| -> Vec<u32> {
        let mut path = Vec::new();
        let mut s = target;
        while let Some((prev, sym)) = parent[s] {
            path.push(sym);
            s = prev;
        }
        path.reverse();
        path
    };

    let sym_name = |id: u32| -> &str { grammar.symbols.name(SymbolId(id)) };
    let sim = ParserSim::new(dfa, lr, nfa_info, grammar);

    let mut results: Vec<crate::table::Conflict> = Vec::new();

    // The raw DFA splits states by lookahead, so many conflicts share the same
    // (prefix, terminal, rules) — the only inputs the example depends on.
    let mut memo: alloc::collections::BTreeMap<ExampleKey, Option<String>> =
        alloc::collections::BTreeMap::new();

    for (source, terminal, kind) in &conflicts {
        let prefix = path_to(*source);
        let terminal_id = terminal.0;

        let (reduce_rule, reduce_rule2) = match kind {
            ConflictKind::ShiftReduce(rule) => (*rule, None),
            ConflictKind::ReduceReduce(r1, r2) => (*r1, Some(*r2)),
        };

        let example = memo
            .entry((prefix.clone(), terminal_id, reduce_rule, reduce_rule2))
            .or_insert_with(|| {
                let ce =
                    find_counterexample(&sim, &prefix, terminal_id, reduce_rule, reduce_rule2)?;
                Some(match &ce {
                    Counterexample::Unifying(conv) => format_convergence(conv, grammar),
                    Counterexample::NonUnifying { suffix_a, suffix_b } => {
                        let join = |syms: &[u32]| -> String {
                            syms.iter()
                                .map(|&s| sym_name(s))
                                .collect::<Vec<_>>()
                                .join(" ")
                        };
                        let pfx = join(&prefix);
                        if reduce_rule2.is_some() {
                            format!(
                                "reduce 1: {} \u{2022} {}\nreduce 2: {} \u{2022} {}",
                                pfx,
                                join(suffix_a),
                                pfx,
                                join(suffix_b),
                            )
                        } else {
                            format!(
                                "shift:  {} \u{2022} {}\nreduce: {} \u{2022} {}",
                                pfx,
                                join(suffix_a),
                                pfx,
                                join(suffix_b),
                            )
                        }
                    }
                })
            })
            .clone();
        let Some(example) = example else { continue };

        let conflict = match kind {
            ConflictKind::ShiftReduce(rule) => crate::table::Conflict::ShiftReduce {
                terminal: *terminal,
                reduce_rule: *rule,
                example,
            },
            ConflictKind::ReduceReduce(r1, r2) => crate::table::Conflict::ReduceReduce {
                terminal: *terminal,
                rule1: *r1,
                rule2: *r2,
                example,
            },
        };
        results.push(conflict);
    }

    results
}

/// Format a CST node. `highlight_rule` causes that rule's Interior to use `[]` instead of `()`.
/// Format a CST node. `is_conflict` nodes use `[]`, others use `()`.
/// Single-child non-conflict Interiors collapse (recurse to child).
fn format_cst(node: &CstNode, grammar: &GrammarInternal) -> String {
    match node {
        CstNode::Leaf { symbol, .. } => grammar.symbols.name(SymbolId(*symbol)).to_string(),
        CstNode::Interior {
            children,
            is_conflict: false,
            ..
        } if children.len() == 1 => format_cst(&children[0], grammar),
        CstNode::Interior {
            children,
            is_conflict,
            ..
        } => {
            let inner: Vec<String> = children.iter().map(|c| format_cst(c, grammar)).collect();
            if *is_conflict {
                format!("[{}]", inner.join(" "))
            } else {
                format!("({})", inner.join(" "))
            }
        }
    }
}

/// Format annotation for a CST: if its top node is the conflict, show `[] → lhs`, else `shift`.
/// Find the `is_conflict` node in a CST and return its annotation.
fn format_annot(node: &CstNode, grammar: &GrammarInternal) -> String {
    match node {
        CstNode::Interior {
            rule,
            is_conflict: true,
            ..
        } => {
            let lhs = grammar.symbols.name(grammar.rules[*rule].lhs.id());
            format!("[] \u{2192} {}", lhs)
        }
        CstNode::Interior { children, .. } => {
            for child in children {
                let annot = format_annot(child, grammar);
                if annot != "shift" {
                    return annot;
                }
            }
            "shift".to_string()
        }
        CstNode::Leaf { .. } => "shift".to_string(),
    }
}

/// Format a convergence for display. `is_conflict` flags are already set on CST nodes.
fn format_convergence(conv: &Convergence, grammar: &GrammarInternal) -> String {
    let nt_name = grammar.symbols.name(SymbolId(conv.nonterminal));

    let line_a = format_cst(&conv.cst_a, grammar);
    let line_b = format_cst(&conv.cst_b, grammar);
    let annot_a = format_annot(&conv.cst_a, grammar);
    let annot_b = format_annot(&conv.cst_b, grammar);

    format!(
        "Ambiguity in {}:\n    {}  {}\n    {}  {}",
        nt_name, line_a, annot_a, line_b, annot_b,
    )
}
