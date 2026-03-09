mod grammar_conflict;
use grammar_conflict::{DfaStateKind, conflict_examples, detect_conflicts, resolve_conflicts};

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::{format, vec, vec::Vec};

use crate::grammar::SymbolId;

/// Convert snake_case or SCREAMING_SNAKE name to CamelCase type name.
/// e.g., "grammar_def" → "GrammarDef", "NAME" → "Name", "COMP_OP" → "CompOp"
pub(crate) fn to_camel_case(name: &str) -> String {
    name.split('_')
        .map(|seg| {
            let mut chars = seg.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().to_string() + &chars.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect()
}

// ============================================================================
// Internal grammar representation
// ============================================================================

/// A grammar symbol: terminal, precedence terminal, or non-terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Symbol {
    /// A regular terminal symbol.
    Terminal(SymbolId),
    /// A terminal that carries precedence at runtime (e.g., operators).
    PrecTerminal(SymbolId),
    /// A non-terminal symbol.
    NonTerminal(SymbolId),
}

impl Symbol {
    pub fn id(&self) -> SymbolId {
        match self {
            Symbol::Terminal(id) | Symbol::PrecTerminal(id) | Symbol::NonTerminal(id) => *id,
        }
    }

    pub fn is_non_terminal(&self) -> bool {
        matches!(self, Symbol::NonTerminal(_))
    }
}

/// Information about a terminal symbol.
#[derive(Debug, Clone)]
pub(crate) struct SymbolInfo {
    pub name: String,
    pub kind: crate::grammar::TerminalKind,
}

/// Symbol table mapping names to IDs and vice versa.
#[derive(Debug, Clone)]
pub(crate) struct SymbolTable {
    /// Terminal info, indexed by id (0..num_terminals). EOF is at index 0.
    terminals: Vec<SymbolInfo>,
    /// Non-terminal names, indexed by id - num_terminals
    non_terminals: Vec<String>,
    /// Lookup from name to Symbol
    name_to_symbol: BTreeMap<String, Symbol>,
    /// Count of terminals (including EOF)
    num_terminals: u32,
}

impl SymbolTable {
    /// Create a new symbol table with EOF already interned as terminal 0.
    pub fn new() -> Self {
        let mut table = Self {
            terminals: Vec::new(),
            non_terminals: Vec::new(),
            name_to_symbol: BTreeMap::new(),
            num_terminals: 0,
        };
        // EOF is always terminal 0
        table.intern_terminal("$");
        table
    }

    /// Intern a terminal symbol with the given kind, returning the Symbol.
    pub fn intern_terminal_with_kind(
        &mut self,
        name: &str,
        kind: crate::grammar::TerminalKind,
    ) -> Symbol {
        if let Some(&sym) = self.name_to_symbol.get(name) {
            return sym;
        }

        let id = SymbolId(self.terminals.len() as u32);
        self.terminals.push(SymbolInfo {
            name: name.to_string(),
            kind,
        });
        let sym = match kind {
            crate::grammar::TerminalKind::Prec | crate::grammar::TerminalKind::Conflict => {
                Symbol::PrecTerminal(id)
            }
            _ => Symbol::Terminal(id),
        };
        self.name_to_symbol.insert(name.to_string(), sym);
        sym
    }

    /// Intern a plain terminal symbol, returning the Symbol.
    pub fn intern_terminal(&mut self, name: &str) -> Symbol {
        self.intern_terminal_with_kind(name, crate::grammar::TerminalKind::Plain)
    }

    /// Intern a non-terminal symbol, returning the Symbol.
    pub fn intern_non_terminal(&mut self, name: &str) -> Symbol {
        if let Some(&sym) = self.name_to_symbol.get(name) {
            return sym;
        }

        let id = SymbolId(self.num_terminals + self.non_terminals.len() as u32);
        self.non_terminals.push(name.to_string());
        let sym = Symbol::NonTerminal(id);
        self.name_to_symbol.insert(name.to_string(), sym);
        sym
    }

    /// Finalize terminal interning. Call this after all terminals are added
    /// and before adding non-terminals.
    pub fn finalize_terminals(&mut self) {
        self.num_terminals = self.terminals.len() as u32;
    }

    /// Get the Symbol for a name, if it exists.
    pub fn get(&self, name: &str) -> Option<Symbol> {
        self.name_to_symbol.get(name).copied()
    }

    /// Get the SymbolId for a name, if it exists.
    pub fn get_id(&self, name: &str) -> Option<SymbolId> {
        self.name_to_symbol.get(name).map(|s| s.id())
    }

    /// Check if this is a terminal (including EOF).
    pub fn is_terminal(&self, id: SymbolId) -> bool {
        id.0 < self.num_terminals
    }

    /// Check if this terminal carries a runtime resolution field (prec or conflict).
    #[cfg(feature = "codegen")]
    pub fn has_resolution_field(&self, id: SymbolId) -> bool {
        matches!(
            self.terminal_kind(id),
            crate::grammar::TerminalKind::Prec | crate::grammar::TerminalKind::Conflict
        )
    }

    /// Get the terminal kind.
    pub fn terminal_kind(&self, id: SymbolId) -> crate::grammar::TerminalKind {
        if id.0 >= self.num_terminals {
            return crate::grammar::TerminalKind::Plain;
        }
        self.terminals[id.0 as usize].kind
    }

    /// Get the name of a symbol.
    pub fn name(&self, id: SymbolId) -> &str {
        if id.0 < self.num_terminals {
            &self.terminals[id.0 as usize].name
        } else {
            let idx = (id.0 - self.num_terminals) as usize;
            &self.non_terminals[idx]
        }
    }

    /// Get the number of terminals (including EOF).
    pub fn num_terminals(&self) -> u32 {
        self.num_terminals
    }

    /// Get the number of non-terminals.
    pub fn num_non_terminals(&self) -> u32 {
        self.non_terminals.len() as u32
    }

    /// Get the total number of symbols.
    pub fn num_symbols(&self) -> u32 {
        self.num_terminals + self.num_non_terminals()
    }

    /// Iterate over all terminal IDs (including EOF at index 0).
    pub fn terminal_ids(&self) -> impl Iterator<Item = SymbolId> {
        (0..self.num_terminals).map(SymbolId)
    }

    /// Iterate over all non-terminal IDs.
    #[cfg(feature = "codegen")]
    pub fn non_terminal_ids(&self) -> impl Iterator<Item = SymbolId> {
        let start = self.num_terminals;
        let end = start + self.non_terminals.len() as u32;
        (start..end).map(SymbolId)
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

/// The action to perform when a rule alternative is reduced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AltAction {
    /// User-defined action name (e.g., `=> binop`).
    Named(String),
    /// Synthetic: wrap value in `Some` (from `?` modifier).
    OptSome,
    /// Synthetic: produce `None` (from `?` modifier).
    OptNone,
    /// Synthetic: create empty `Vec` (from `*` modifier).
    VecEmpty,
    /// Synthetic: create `Vec` with single element (from `+`, `*`, `%` modifiers).
    VecSingle,
    /// Synthetic: append last element to `Vec` (from `+`, `*`, `%` modifiers).
    VecAppend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rule {
    pub lhs: Symbol,
    pub rhs: Vec<Symbol>,
    /// Action to perform when this rule is reduced.
    pub action: AltAction,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // `types` is used by codegen (behind cfg)
pub(crate) struct GrammarInternal {
    pub rules: Vec<Rule>,
    pub symbols: SymbolTable,
    /// Type for each symbol (terminal payload or non-terminal result). None = unit type.
    pub types: BTreeMap<SymbolId, Option<String>>,
}

impl GrammarInternal {
    /// Returns all rules with the given non-terminal on the left-hand side.
    pub fn rules_for(&self, symbol: Symbol) -> impl Iterator<Item = (usize, &Rule)> {
        self.rules
            .iter()
            .enumerate()
            .filter(move |(_, rule)| rule.lhs == symbol)
    }
}

// ============================================================================
// Grammar conversion (AST -> Internal)
// ============================================================================

use crate::grammar::{Grammar, Term};

/// Convert Grammar AST to internal representation.
///
/// Desugars modifier symbols (?, *, +, %) into synthetic helper rules
/// with proper [`AltAction`]s, then builds the augmented grammar.
pub(crate) fn to_grammar_internal(grammar: &Grammar) -> Result<GrammarInternal, String> {
    if grammar.rules.is_empty() {
        return Err("Grammar has no rules".to_string());
    }

    let mut symbols = SymbolTable::new();
    let mut types: BTreeMap<SymbolId, Option<String>> = BTreeMap::new();

    // Register terminals + types
    for def in &grammar.terminals {
        let sym = symbols.intern_terminal_with_kind(&def.name, def.kind);
        let type_name = if def.has_type {
            Some(to_camel_case(&def.name))
        } else {
            None
        };
        types.insert(sym.id(), type_name);
    }
    symbols.finalize_terminals();

    // Register user non-terminals + types
    // Every NT gets an auto-derived associated type from its name
    for rule in &grammar.rules {
        let sym = symbols.intern_non_terminal(&rule.name);
        types.insert(sym.id(), Some(to_camel_case(&rule.name)));
    }

    // Build rules, desugaring modifier terms inline
    let mut desugared: BTreeMap<Term, Symbol> = BTreeMap::new();
    let mut rules = Vec::new();

    for rule in &grammar.rules {
        let lhs = symbols.get(&rule.name).unwrap();

        for alt in &rule.alts {
            let has_empty = alt.terms.iter().any(|t| matches!(t, Term::Empty));

            let rhs: Vec<Symbol> = if has_empty {
                Vec::new()
            } else {
                alt.terms
                    .iter()
                    .map(|term| {
                        resolve_term(term, &mut symbols, &mut types, &mut desugared, &mut rules)
                            .map_err(|e| format!("{e} (in rule '{}')", rule.name))
                    })
                    .collect::<Result<Vec<_>, _>>()?
            };

            let action = AltAction::Named(alt.name.clone());

            rules.push(Rule { lhs, rhs, action });
        }
    }

    // Augment with __start -> <original_start>
    let start = symbols
        .get(&grammar.start)
        .ok_or_else(|| format!("Start symbol '{}' not found in grammar", grammar.start))?;
    let aug_start = symbols.intern_non_terminal("__start");
    let aug_rule = Rule {
        lhs: aug_start,
        rhs: vec![start],
        action: AltAction::Named(String::new()),
    };
    let mut aug_rules = vec![aug_rule];
    aug_rules.extend(rules);

    Ok(GrammarInternal {
        rules: aug_rules,
        symbols,
        types,
    })
}

fn resolve(symbols: &SymbolTable, name: &str) -> Result<Symbol, String> {
    symbols
        .get(name)
        .ok_or_else(|| format!("Unknown symbol: {}", name))
}

fn resolve_term(
    term: &Term,
    symbols: &mut SymbolTable,
    types: &mut BTreeMap<SymbolId, Option<String>>,
    desugared: &mut BTreeMap<Term, Symbol>,
    rules: &mut Vec<Rule>,
) -> Result<Symbol, String> {
    if let Term::Symbol(name) = term {
        return resolve(symbols, name);
    }
    if let Some(&sym) = desugared.get(term) {
        return Ok(sym);
    }
    let lhs = match term {
        Term::Optional(name) => {
            let lhs = symbols.intern_non_terminal(&format!("__{}_opt", name.to_lowercase()));
            let inner = lookup_type(name, symbols, types);
            types.insert(lhs.id(), inner.map(|t| format!("Option<{}>", t)));
            let sym = resolve(symbols, name)?;
            rules.push(Rule {
                lhs,
                rhs: vec![sym],
                action: AltAction::OptSome,
            });
            rules.push(Rule {
                lhs,
                rhs: vec![],
                action: AltAction::OptNone,
            });
            lhs
        }
        Term::ZeroOrMore(name) => {
            let lhs = symbols.intern_non_terminal(&format!("__{}_star", name.to_lowercase()));
            let inner = lookup_type(name, symbols, types);
            types.insert(lhs.id(), inner.map(|t| format!("Vec<{}>", t)));
            let sym = resolve(symbols, name)?;
            rules.push(Rule {
                lhs,
                rhs: vec![lhs, sym],
                action: AltAction::VecAppend,
            });
            rules.push(Rule {
                lhs,
                rhs: vec![],
                action: AltAction::VecEmpty,
            });
            lhs
        }
        Term::OneOrMore(name) => {
            let lhs = symbols.intern_non_terminal(&format!("__{}_plus", name.to_lowercase()));
            let inner = lookup_type(name, symbols, types);
            types.insert(lhs.id(), inner.map(|t| format!("Vec<{}>", t)));
            let sym = resolve(symbols, name)?;
            rules.push(Rule {
                lhs,
                rhs: vec![lhs, sym],
                action: AltAction::VecAppend,
            });
            rules.push(Rule {
                lhs,
                rhs: vec![sym],
                action: AltAction::VecSingle,
            });
            lhs
        }
        Term::SeparatedBy { symbol, sep } => {
            let lhs = symbols.intern_non_terminal(&format!(
                "__{}_sep_{}",
                symbol.to_lowercase(),
                sep.to_lowercase()
            ));
            let inner = lookup_type(symbol, symbols, types);
            types.insert(lhs.id(), inner.map(|t| format!("Vec<{}>", t)));
            let sym = resolve(symbols, symbol)?;
            let sep_sym = resolve(symbols, sep)?;
            rules.push(Rule {
                lhs,
                rhs: vec![lhs, sep_sym, sym],
                action: AltAction::VecAppend,
            });
            rules.push(Rule {
                lhs,
                rhs: vec![sym],
                action: AltAction::VecSingle,
            });
            lhs
        }
        Term::Symbol(_) | Term::Empty => unreachable!(),
    };
    desugared.insert(term.clone(), lhs);
    Ok(lhs)
}

fn lookup_type(
    name: &str,
    symbols: &SymbolTable,
    types: &BTreeMap<SymbolId, Option<String>>,
) -> Option<String> {
    let id = symbols.get_id(name)?;
    let ty = types.get(&id)?;
    Some(ty.as_deref().unwrap_or("()").to_string())
}

// ============================================================================
// LR parsing
// ============================================================================

/// A bitset representing a set of terminals (including EOF at bit 0).
#[derive(Debug, Clone, PartialEq, Eq)]
struct TerminalSet {
    bits: Vec<u64>,
    /// Whether this set can derive epsilon (empty string).
    pub has_epsilon: bool,
}

impl TerminalSet {
    /// Create an empty terminal set.
    pub fn new(num_terminals: u32) -> Self {
        let num_bits = num_terminals as usize;
        let num_words = num_bits.div_ceil(64);
        Self {
            bits: vec![0; num_words],
            has_epsilon: false,
        }
    }

    /// Insert a terminal ID into the set.
    pub fn insert(&mut self, id: SymbolId) -> bool {
        let idx = id.0 as usize;
        let word = idx / 64;
        let bit = idx % 64;
        if word < self.bits.len() {
            let mask = 1u64 << bit;
            let was_set = (self.bits[word] & mask) != 0;
            self.bits[word] |= mask;
            !was_set
        } else {
            false
        }
    }

    #[cfg(test)]
    fn contains(&self, id: SymbolId) -> bool {
        let idx = id.0 as usize;
        let word = idx / 64;
        let bit = idx % 64;
        if word < self.bits.len() {
            (self.bits[word] & (1u64 << bit)) != 0
        } else {
            false
        }
    }

    /// Iterate over all terminal IDs in the set.
    pub fn iter(&self) -> impl Iterator<Item = SymbolId> + '_ {
        self.bits.iter().enumerate().flat_map(|(word_idx, &word)| {
            (0..64).filter_map(move |bit| {
                if (word & (1u64 << bit)) != 0 {
                    Some(SymbolId((word_idx * 64 + bit) as u32))
                } else {
                    None
                }
            })
        })
    }
}

/// FIRST sets for all symbols, indexed by SymbolId.
#[derive(Debug, Clone)]
struct FirstSets {
    /// FIRST set for each symbol, indexed by symbol ID.
    sets: Vec<TerminalSet>,
    num_terminals: u32,
}

impl FirstSets {
    /// Compute FIRST sets for a grammar.
    pub fn compute(grammar: &GrammarInternal) -> Self {
        let num_terminals = grammar.symbols.num_terminals();
        let num_symbols = grammar.symbols.num_symbols();

        let mut sets: Vec<TerminalSet> = (0..num_symbols)
            .map(|_| TerminalSet::new(num_terminals))
            .collect();

        // Initialize: terminals have FIRST = {self}
        for id in grammar.symbols.terminal_ids() {
            sets[id.0 as usize].insert(id);
        }

        // Fixed-point iteration for non-terminals
        let mut changed = true;
        while changed {
            changed = false;

            for rule in &grammar.rules {
                let lhs = rule.lhs.id();
                let rhs_ids: Vec<SymbolId> = rule.rhs.iter().map(|s| s.id()).collect();
                let rhs_first =
                    Self::first_of_sequence(&rhs_ids, &sets, num_terminals, &grammar.symbols);

                // Add all terminals from rhs_first to sets[lhs]
                for id in rhs_first.iter() {
                    if sets[lhs.0 as usize].insert(id) {
                        changed = true;
                    }
                }
                if rhs_first.has_epsilon && !sets[lhs.0 as usize].has_epsilon {
                    sets[lhs.0 as usize].has_epsilon = true;
                    changed = true;
                }
            }
        }

        FirstSets {
            sets,
            num_terminals,
        }
    }

    /// Compute FIRST of a sequence of symbols.
    fn first_of_sequence(
        symbols: &[SymbolId],
        sets: &[TerminalSet],
        num_terminals: u32,
        symbol_table: &SymbolTable,
    ) -> TerminalSet {
        let mut result = TerminalSet::new(num_terminals);

        if symbols.is_empty() {
            result.has_epsilon = true;
            return result;
        }

        for &sym in symbols {
            if symbol_table.is_terminal(sym) {
                // Terminal: add it and stop
                result.insert(sym);
                return result;
            }

            // Non-terminal: add its FIRST set (excluding epsilon)
            let sym_first = &sets[sym.0 as usize];
            for id in sym_first.iter() {
                result.insert(id);
            }

            // If this non-terminal can't derive epsilon, stop
            if !sym_first.has_epsilon {
                return result;
            }
        }

        // All symbols can derive epsilon
        result.has_epsilon = true;
        result
    }

    #[cfg(test)]
    fn get(&self, id: SymbolId) -> &TerminalSet {
        &self.sets[id.0 as usize]
    }

    /// Compute FIRST of a sequence followed by a lookahead.
    pub fn first_of_sequence_with_lookahead(
        &self,
        symbols: &[SymbolId],
        lookahead: SymbolId,
        symbol_table: &SymbolTable,
    ) -> TerminalSet {
        let mut result = TerminalSet::new(self.num_terminals);

        for &sym in symbols {
            if symbol_table.is_terminal(sym) {
                result.insert(sym);
                return result;
            }

            let sym_first = &self.sets[sym.0 as usize];
            for id in sym_first.iter() {
                result.insert(id);
            }

            if !sym_first.has_epsilon {
                return result;
            }
        }

        // All symbols can derive epsilon, add the lookahead
        result.insert(lookahead);
        result
    }
}

///// An LR(1) item: a rule with a dot position and lookahead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Item {
    /// Index of the rule in the grammar.
    pub(crate) rule: usize,
    /// Position of the dot (0 = before first symbol, len = after last).
    pub(crate) dot: usize,
    /// Lookahead terminal ID (EOF = SymbolId(0)).
    pub(crate) lookahead: SymbolId,
}

impl Item {
    fn new(rule: usize, dot: usize, lookahead: SymbolId) -> Self {
        Self {
            rule,
            dot,
            lookahead,
        }
    }
}

// ============================================================================
// NFA → DFA → Hopcroft minimization pipeline
// ============================================================================

use crate::automaton::{self, Dfa};

/// LR-specific metadata kept alongside the generic NFA.
/// NFA states 0..items.len() are item nodes.
/// NFA states items.len()..items.len()+num_rules are reduce/accept nodes.
pub(crate) struct LrNfaInfo {
    pub(crate) items: Vec<Item>,
    /// Reverse mapping: virtual reduce ID -> real terminal ID
    pub(crate) reduce_to_real: BTreeMap<u32, u32>,
    /// Forward mapping: real terminal ID -> virtual reduce ID
    pub(crate) real_to_virtual: BTreeMap<u32, u32>,
}

/// LR-specific metadata derived from DFA state classification.
pub(crate) struct DfaLrInfo {
    /// For each DFA state: reduce rules present (empty if pure item state)
    pub(crate) reduce_rules: Vec<Vec<usize>>,
    /// NFA item indices per state (for error reporting)
    pub(crate) nfa_items: Vec<Vec<usize>>,
}

impl DfaLrInfo {
    pub(crate) fn has_items(&self, state: usize) -> bool {
        !self.nfa_items[state].is_empty()
    }
}

/// Classify DFA states by inspecting which NFA states they contain.
fn classify_dfa_states(nfa_sets: &[Vec<usize>], num_items: usize) -> DfaLrInfo {
    let mut reduce_rules = vec![Vec::new(); nfa_sets.len()];
    let mut nfa_items = vec![Vec::new(); nfa_sets.len()];

    for (idx, nfa_set) in nfa_sets.iter().enumerate() {
        for &nfa_state in nfa_set {
            if nfa_state >= num_items {
                reduce_rules[idx].push(nfa_state - num_items);
            } else {
                nfa_items[idx].push(nfa_state);
            }
        }
        reduce_rules[idx].sort();
        reduce_rules[idx].dedup();
    }

    DfaLrInfo {
        reduce_rules,
        nfa_items,
    }
}

/// Build prec terminal mapping: real terminal ID → virtual reduce symbol ID.
/// Returns (prec_to_reduce, reduce_to_real).
fn build_prec_mapping(grammar: &GrammarInternal) -> (Vec<Option<u32>>, BTreeMap<u32, u32>) {
    let num_terminals = grammar.symbols.num_terminals() as usize;
    let mut prec_to_reduce: Vec<Option<u32>> = vec![None; num_terminals];
    let mut reduce_to_real: BTreeMap<u32, u32> = BTreeMap::new();
    let mut next_virtual = grammar.symbols.num_symbols();
    for id in grammar.symbols.terminal_ids() {
        if grammar.symbols.terminal_kind(id) != crate::grammar::TerminalKind::Plain {
            prec_to_reduce[id.0 as usize] = Some(next_virtual);
            reduce_to_real.insert(next_virtual, id.0);
            next_virtual += 1;
        }
    }
    (prec_to_reduce, reduce_to_real)
}

/// Build LR(1) NFA by enumerating all (rule, dot, lookahead) triples.
/// Unreachable states are harmless — subset construction ignores them.
fn build_lr_nfa(grammar: &GrammarInternal, first_sets: &FirstSets) -> (automaton::Nfa, LrNfaInfo) {
    let num_rules = grammar.rules.len();
    let num_terminals = grammar.symbols.num_terminals();
    let (prec_to_reduce, reduce_to_real) = build_prec_mapping(grammar);
    let real_to_virtual: BTreeMap<u32, u32> =
        reduce_to_real.iter().map(|(&v, &r)| (r, v)).collect();

    // Compute flat index: item(rule, dot, la) and total item count
    let mut rule_offsets: Vec<usize> = Vec::with_capacity(num_rules);
    let mut num_items = 0;
    for rule in &grammar.rules {
        rule_offsets.push(num_items);
        num_items += (rule.rhs.len() + 1) * num_terminals as usize;
    }

    let item_state = |rule: usize, dot: usize, la: u32| -> usize {
        rule_offsets[rule] + dot * num_terminals as usize + la as usize
    };

    // Build items list for later classification
    let mut items: Vec<Item> = vec![Item::new(0, 0, SymbolId::EOF); num_items];
    for (rule_idx, rule) in grammar.rules.iter().enumerate() {
        for dot in 0..=rule.rhs.len() {
            for la in 0..num_terminals {
                let idx = item_state(rule_idx, dot, la);
                items[idx] = Item::new(rule_idx, dot, SymbolId(la));
            }
        }
    }

    let mut nfa = automaton::Nfa::new();
    // Pre-allocate: item states + reduce nodes
    for _ in 0..(num_items + num_rules) {
        nfa.add_state();
    }

    for (rule_idx, rule) in grammar.rules.iter().enumerate() {
        for dot in 0..=rule.rhs.len() {
            for la in 0..num_terminals {
                let idx = item_state(rule_idx, dot, la);

                if dot == rule.rhs.len() {
                    // Complete: transition on lookahead to reduce node
                    let reduce_node = num_items + rule_idx;
                    let sym = prec_to_reduce
                        .get(la as usize)
                        .and_then(|x| *x)
                        .unwrap_or(la);
                    nfa.add_transition(idx, sym, reduce_node);
                } else {
                    let next_sym = rule.rhs[dot];
                    let advanced = item_state(rule_idx, dot + 1, la);
                    nfa.add_transition(idx, next_sym.id().0, advanced);

                    if next_sym.is_non_terminal() {
                        let beta: Vec<SymbolId> =
                            rule.rhs[dot + 1..].iter().map(|s| s.id()).collect();
                        let lookaheads = first_sets.first_of_sequence_with_lookahead(
                            &beta,
                            SymbolId(la),
                            &grammar.symbols,
                        );
                        for (closure_rule, _) in grammar.rules_for(next_sym) {
                            for closure_la in lookaheads.iter() {
                                let target = item_state(closure_rule, 0, closure_la.0);
                                nfa.add_epsilon(idx, target);
                            }
                        }
                    }
                }
            }
        }
    }

    (
        nfa,
        LrNfaInfo {
            items,
            reduce_to_real,
            real_to_virtual,
        },
    )
}

/// Result of the automaton construction pipeline.
pub(crate) struct AutomatonResult {
    /// Permuted DFA: states [0, num_item_states) are item states,
    /// state num_item_states + r reduces rule r.
    pub dfa: Dfa,
    pub num_item_states: usize,
    pub state_items: Vec<Vec<(u16, u8)>>,
    pub conflicts: Vec<crate::table::Conflict>,
    /// Virtual reduce symbol → real terminal ID (for prec terminals).
    pub reduce_to_real: BTreeMap<u32, u32>,
}

/// For each group of DFA states with the same LR(0) core, copy reduce
/// transitions from siblings to fill gaps. This makes same-core states
/// identical (when they don't conflict), so Hopcroft merges them — achieving
/// LALR-style minimization.
fn merge_lookaheads(dfa: &mut Dfa, states: &[DfaStateKind]) {
    // Group internal states by core: sorted (rule, dot) pairs
    let mut core_groups: BTreeMap<Vec<(usize, usize)>, Vec<usize>> = BTreeMap::new();
    for (state, kind) in states.iter().enumerate() {
        if let DfaStateKind::Items(items) = kind {
            let mut core = items.clone();
            core.sort();
            core.dedup();
            core_groups.entry(core).or_default().push(state);
        }
    }

    for group in core_groups.values() {
        if group.len() <= 1 {
            continue;
        }
        // Collect reduce transitions: only keep if all states that have
        // the transition agree on the target
        let mut sym_to_target: BTreeMap<u32, Option<usize>> = BTreeMap::new();
        for &state in group {
            for &(sym, target) in &dfa.transitions[state] {
                if matches!(states[target], DfaStateKind::Reduce(_)) {
                    sym_to_target
                        .entry(sym)
                        .and_modify(|t| {
                            if *t != Some(target) {
                                *t = None
                            }
                        })
                        .or_insert(Some(target));
                }
            }
        }

        // Fill gaps: add transitions each state is missing
        for &state in group {
            let existing: BTreeSet<u32> =
                dfa.transitions[state].iter().map(|&(sym, _)| sym).collect();
            for (&sym, &target) in &sym_to_target {
                if let Some(target) = target
                    && !existing.contains(&sym)
                {
                    dfa.transitions[state].push((sym, target));
                }
            }
        }
    }
}

/// Build a minimal LR(1) automaton for a grammar using NFA → DFA → Hopcroft.
pub(crate) fn build_minimal_automaton(grammar: &GrammarInternal) -> AutomatonResult {
    let first_sets = FirstSets::compute(grammar);
    let (nfa, nfa_info) = build_lr_nfa(grammar, &first_sets);
    let num_items = nfa_info.items.len();

    let (mut raw_dfa, raw_nfa_sets) = automaton::subset_construction(&nfa);
    let dfa_lr_info = classify_dfa_states(&raw_nfa_sets, num_items);
    let dfa_conflicts = detect_conflicts(&raw_dfa, &dfa_lr_info, &nfa_info, grammar);
    let conflicts = conflict_examples(&raw_dfa, &dfa_lr_info, &nfa_info, grammar, dfa_conflicts);
    let resolved = resolve_conflicts(dfa_lr_info, &nfa_info);
    merge_lookaheads(&mut raw_dfa, &resolved);

    // Initial partition for Hopcroft: reduce states grouped by rule,
    // all item states in one partition. (Reduce states are leaves — Hopcroft
    // can't distinguish them by transitions alone.)
    let num_rules = grammar.rules.len();
    let initial_partition: Vec<usize> = resolved
        .iter()
        .map(|kind| match kind {
            DfaStateKind::Reduce(rule) => *rule,
            DfaStateKind::Items(_) => num_rules,
        })
        .collect();

    let (min_dfa, state_map) = automaton::hopcroft_minimize(&raw_dfa, &initial_partition);

    // Map resolved classification through Hopcroft's state_map.
    let mut min_states = vec![const { DfaStateKind::Reduce(0) }; min_dfa.num_states()];
    for (raw_state, kind) in resolved.into_iter().enumerate() {
        min_states[state_map[raw_state]] = kind;
    }

    // Permute: item states first [0, num_item_states), then reduce states.
    // Reduce state for rule r = num_item_states + r.
    let mut permutation = vec![0usize; min_dfa.num_states()];
    let mut num_item_states = 0;
    for (state, kind) in min_states.iter().enumerate() {
        if let DfaStateKind::Items(_) = kind {
            permutation[state] = num_item_states;
            num_item_states += 1;
        }
    }
    for (state, kind) in min_states.iter().enumerate() {
        if let DfaStateKind::Reduce(rule) = kind {
            permutation[state] = num_item_states + rule;
        }
    }

    let total_states = num_item_states + num_rules;
    let mut new_transitions = vec![Vec::new(); total_states];
    for (old_state, trans) in min_dfa.transitions.iter().enumerate() {
        let new_state = permutation[old_state];
        new_transitions[new_state] = trans
            .iter()
            .map(|&(sym, target)| (sym, permutation[target]))
            .collect();
    }

    let permuted_dfa = Dfa {
        transitions: new_transitions,
    };

    let mut state_items = vec![Vec::new(); num_item_states];
    for (state, kind) in min_states.iter().enumerate() {
        if let DfaStateKind::Items(items) = kind {
            state_items[permutation[state]] = items
                .iter()
                .map(|&(rule, dot)| (rule as u16, dot as u8))
                .collect();
        }
    }

    AutomatonResult {
        dfa: permuted_dfa,
        num_item_states,
        state_items,
        conflicts,
        reduce_to_real: nfa_info.reduce_to_real,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::parse_grammar;
    fn expr_grammar() -> GrammarInternal {
        to_grammar_internal(
            &parse_grammar(
                r#"
            start expr;
            terminals { PLUS, NUM }
            expr = expr PLUS term => add | term => term;
            term = NUM => num;
        "#,
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn test_terminal_set() {
        let mut set = TerminalSet::new(10);

        assert!(!set.contains(SymbolId(0)));
        assert!(!set.contains(SymbolId(5)));

        set.insert(SymbolId(0)); // EOF
        set.insert(SymbolId(5));

        assert!(set.contains(SymbolId(0)));
        assert!(set.contains(SymbolId(5)));
        assert!(!set.contains(SymbolId(3)));

        let ids: Vec<_> = set.iter().collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&SymbolId(0)));
        assert!(ids.contains(&SymbolId(5)));
    }

    #[test]
    fn test_first_sets() {
        let grammar = expr_grammar();
        let first_sets = FirstSets::compute(&grammar);

        // Get symbol IDs
        let num_id = grammar.symbols.get_id("NUM").unwrap();
        let term_id = grammar.symbols.get_id("term").unwrap();
        let expr_id = grammar.symbols.get_id("expr").unwrap();

        // FIRST(term) = {NUM}
        let term_first = first_sets.get(term_id);
        assert!(term_first.contains(num_id));

        // FIRST(expr) = {NUM}
        let expr_first = first_sets.get(expr_id);
        assert!(expr_first.contains(num_id));
    }

    #[test]
    fn test_paren_grammar() {
        let grammar = to_grammar_internal(
            &parse_grammar(
                r#"
            start expr;
            terminals { NUM, LPAREN, RPAREN }
            expr = NUM => num | LPAREN expr RPAREN => paren;
        "#,
            )
            .unwrap(),
        )
        .unwrap();

        use crate::table::CompiledTable;
        let compiled = CompiledTable::build_from_internal(&grammar);

        assert!(!compiled.has_conflicts());
    }

    /// Count LALR states by computing LR(0) core equivalence classes
    /// on the raw DFA item states.
    fn lalr_state_count(grammar: &GrammarInternal) -> usize {
        let first_sets = FirstSets::compute(grammar);
        let (nfa, nfa_info) = build_lr_nfa(grammar, &first_sets);
        let num_items = nfa_info.items.len();
        let (raw_dfa, raw_nfa_sets) = crate::automaton::subset_construction(&nfa);
        let lr = classify_dfa_states(&raw_nfa_sets, num_items);

        let num_terminals = grammar.symbols.num_terminals() as usize;

        // For each item-bearing DFA state, compute its LR(0) core
        // (the set of (rule, dot) pairs, stripping lookahead).
        let mut cores: alloc::collections::BTreeSet<Vec<usize>> =
            alloc::collections::BTreeSet::new();
        for (state, nfa_set) in raw_nfa_sets.iter().enumerate().take(raw_dfa.num_states()) {
            if !lr.has_items(state) {
                continue;
            }
            let mut core: Vec<usize> = nfa_set
                .iter()
                .filter(|&&s| s < num_items)
                .map(|&s| s / num_terminals)
                .collect();
            core.sort();
            core.dedup();
            cores.insert(core);
        }
        cores.len()
    }

    fn grammar_stats(grammar: &GrammarInternal) -> (usize, usize, usize, usize) {
        use crate::table::CompiledTable;
        let compiled = CompiledTable::build_from_internal(grammar);
        let rr = compiled
            .conflicts
            .iter()
            .filter(|c| matches!(c, crate::table::Conflict::ReduceReduce { .. }))
            .count();
        let sr = compiled
            .conflicts
            .iter()
            .filter(|c| matches!(c, crate::table::Conflict::ShiftReduce { .. }))
            .count();
        (compiled.num_states, lalr_state_count(grammar), rr, sr)
    }

    #[test]
    fn print_state_counts() {
        let grammars = [
            ("C11", "grammars/c11.gzl"),
            ("Python", "grammars/python.gzl"),
            ("Meta", "grammars/meta.gzl"),
        ];

        for (name, path) in grammars {
            let src = std::fs::read_to_string(path).unwrap();
            let grammar = to_grammar_internal(&parse_grammar(&src).unwrap()).unwrap();
            let (min_lr, lalr, rr, sr) = grammar_stats(&grammar);
            std::eprintln!(
                "{}: minimal LR {} states, LALR {} states, {} rr, {} sr",
                name,
                min_lr,
                lalr,
                rr,
                sr
            );
            // LALR grammars: minimal LR must equal LALR
            assert_eq!(min_lr, lalr, "{} should have same state count", name);
        }

        // C11 has known conflicts (one per (state, terminal) pair)
        let c11_src = std::fs::read_to_string("grammars/c11.gzl").unwrap();
        let c11 = to_grammar_internal(&parse_grammar(&c11_src).unwrap()).unwrap();
        let (_, _, rr, sr) = grammar_stats(&c11);
        assert_eq!(rr, 3);
        assert_eq!(sr, 0);

        // Classic LR(1)-but-not-LALR(1) grammar:
        // S → aEa | bEb | aFb | bFa; E → e; F → e
        // LALR merges the states for "E → e•" and "F → e•" (same core)
        // but they have incompatible lookaheads (a vs b), causing a
        // spurious reduce/reduce conflict.
        let non_lalr = to_grammar_internal(
            &parse_grammar(
                r#"
            start s;
            terminals { a, b, e }
            s = a ee a => aea | b ee b => beb | a f b => afb | b f a => bfa;
            ee = e => e;
            f = e => f;
        "#,
            )
            .unwrap(),
        )
        .unwrap();
        let (min_lr, lalr, rr, sr) = grammar_stats(&non_lalr);
        std::eprintln!(
            "non-LALR: minimal LR {} states, LALR {} states, {} rr, {} sr",
            min_lr,
            lalr,
            rr,
            sr
        );
        // Minimal LR must have MORE states than LALR (it splits to avoid conflicts)
        assert!(
            min_lr > lalr,
            "minimal LR should have more states than LALR"
        );
        // And no conflicts
        assert_eq!(rr, 0, "minimal LR should have no reduce/reduce conflicts");
        assert_eq!(sr, 0, "minimal LR should have no shift/reduce conflicts");
    }
}
