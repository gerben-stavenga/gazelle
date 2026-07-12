//! Integration tests for the gazelle! macro.

use gazelle::Action;
use gazelle_macros::gazelle;

// Define a simple grammar for testing with the trait-based API
gazelle! {
    grammar simple {
        start s;
        terminals {
            A
        }

        s = A => make_unit;
    }
}

// Implement the actions trait
struct SimpleActionsImpl;

impl gazelle::ErrorType for SimpleActionsImpl {
    type Error = core::convert::Infallible;
}

impl simple::Types for SimpleActionsImpl {
    type S = ();
}

impl Action<simple::S<Self>> for SimpleActionsImpl {
    fn build(&mut self, _node: simple::S<Self>) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[test]
fn test_simple_grammar_types() {
    let mut parser = simple::Parser::<SimpleActionsImpl>::new();
    let mut actions = SimpleActionsImpl;

    // Push the terminal - this handles reduction internally
    parser = parser.push(simple::Terminal::A, &mut actions).unwrap();

    // Finish and get result
    parser.finish(&mut actions).unwrap();
}

#[test]
fn generated_recovery_consumes_semantic_parser() {
    let mut parser = simple::Parser::<SimpleActionsImpl>::new();
    let mut actions = SimpleActionsImpl;

    parser = parser.push(simple::Terminal::A, &mut actions).unwrap();
    let error = match parser.push(simple::Terminal::A, &mut actions) {
        Ok(_) => panic!("unexpected token was accepted"),
        Err(error) => error,
    };

    let diagnostic = simple::diagnose_error(&error).unwrap();
    assert_eq!(
        diagnostic.unexpected,
        simple::Terminal::<SimpleActionsImpl>::A.symbol_id()
    );
    assert_eq!(diagnostic.position, 1);
    assert_eq!(diagnostic.expected, vec![gazelle::SymbolId::EOF]);
    assert_eq!(simple::symbol_name(diagnostic.unexpected), "A");
    assert!(
        simple::format_error(&error, None, None)
            .unwrap()
            .contains("unexpected 'A'")
    );
    assert!(!diagnostic.stack.is_empty());
    assert!(!diagnostic.contexts.is_empty());

    let unexpected = gazelle::Token::new(simple::Terminal::<SimpleActionsImpl>::A.symbol_id());
    let mut recovery = error.into_recovery();
    let errors = recovery.recover(&[unexpected]);
    assert!(!errors.is_empty());
    assert!(recovery.recover(&[]).is_empty());
}

// Test a grammar with payload types
gazelle! {
    pub grammar num_parser {
        start value;
        terminals {
            NUM: _
        }

        value = NUM => identity;
    }
}

struct NumActionsImpl;

impl gazelle::ErrorType for NumActionsImpl {
    type Error = core::convert::Infallible;
}

impl num_parser::Types for NumActionsImpl {
    type Num = i32;
    type Value = i32;
}

impl Action<num_parser::Value<Self>> for NumActionsImpl {
    fn build(&mut self, node: num_parser::Value<Self>) -> Result<i32, Self::Error> {
        let num_parser::Value::Identity(n) = node;
        Ok(n)
    }
}

#[test]
fn test_payload_grammar() {
    let mut parser = num_parser::Parser::<NumActionsImpl>::new();
    let mut actions = NumActionsImpl;

    // Push the terminal
    parser = parser
        .push(num_parser::Terminal::Num(42), &mut actions)
        .unwrap();

    // Finish and get result
    let result = parser.finish(&mut actions).unwrap();
    assert_eq!(result, 42);
}

// Test a more complex grammar with named reductions
gazelle! {
    grammar expr {
        start expr;
        terminals {
            NUM: _,
            PLUS
        }

        expr = expr PLUS term => add
             | term => term_to_expr;

        term = NUM => literal;
    }
}

struct ExprActionsImpl;

impl gazelle::ErrorType for ExprActionsImpl {
    type Error = core::convert::Infallible;
}

impl expr::Types for ExprActionsImpl {
    type Num = i32;
    type Expr = i32;
    type Term = i32;
}

impl Action<expr::Expr<Self>> for ExprActionsImpl {
    fn build(&mut self, node: expr::Expr<Self>) -> Result<i32, Self::Error> {
        Ok(match node {
            expr::Expr::Add(left, right) => left + right,
            expr::Expr::TermToExpr(t) => t,
        })
    }
}

impl Action<expr::Term<Self>> for ExprActionsImpl {
    fn build(&mut self, node: expr::Term<Self>) -> Result<i32, Self::Error> {
        let expr::Term::Literal(n) = node;
        Ok(n)
    }
}

#[test]
fn test_expr_grammar() {
    let mut parser = expr::Parser::<ExprActionsImpl>::new();
    let mut actions = ExprActionsImpl;

    // Parse: 1 + 2
    parser = parser.push(expr::Terminal::Num(1), &mut actions).unwrap();
    parser = parser.push(expr::Terminal::Plus, &mut actions).unwrap();
    parser = parser.push(expr::Terminal::Num(2), &mut actions).unwrap();

    let result = parser.finish(&mut actions).unwrap();
    assert_eq!(result, 3);
}

// Test set_token_range callback
struct SpanTracker {
    spans: Vec<(usize, usize)>,
}

impl gazelle::ErrorType for SpanTracker {
    type Error = core::convert::Infallible;
}

impl expr::Types for SpanTracker {
    type Num = i32;
    type Expr = i32;
    type Term = i32;

    fn set_token_range(&mut self, start: usize, end: usize) {
        self.spans.push((start, end));
    }
}

impl Action<expr::Expr<Self>> for SpanTracker {
    fn build(&mut self, node: expr::Expr<Self>) -> Result<i32, Self::Error> {
        Ok(match node {
            expr::Expr::Add(left, right) => left + right,
            expr::Expr::TermToExpr(t) => t,
        })
    }
}

impl Action<expr::Term<Self>> for SpanTracker {
    fn build(&mut self, node: expr::Term<Self>) -> Result<i32, Self::Error> {
        let expr::Term::Literal(n) = node;
        Ok(n)
    }
}

#[test]
fn test_set_token_range() {
    let mut parser = expr::Parser::<SpanTracker>::new();
    let mut actions = SpanTracker { spans: Vec::new() };

    // Parse: 1 + 2   (tokens at indices 0, 1, 2)
    parser = parser.push(expr::Terminal::Num(1), &mut actions).unwrap();
    parser = parser.push(expr::Terminal::Plus, &mut actions).unwrap();
    parser = parser.push(expr::Terminal::Num(2), &mut actions).unwrap();

    let result = parser.finish(&mut actions).unwrap();
    assert_eq!(result, 3);

    // Reductions: term(0,1), expr(0,1), term(2,3), expr(0,3)
    assert!(
        actions.spans.contains(&(0, 1)),
        "term '1' span: {:?}",
        actions.spans
    );
    assert!(
        actions.spans.contains(&(2, 3)),
        "term '2' span: {:?}",
        actions.spans
    );
    assert!(
        actions.spans.contains(&(0, 3)),
        "expr '1+2' span: {:?}",
        actions.spans
    );
}

// Test separator list (%)
gazelle! {
    grammar csv_list {
        start items;
        terminals {
            NUM: _,
            COMMA
        }

        items = (NUM % COMMA) => items;
    }
}

struct CsvActionsImpl;

impl gazelle::ErrorType for CsvActionsImpl {
    type Error = core::convert::Infallible;
}

impl csv_list::Types for CsvActionsImpl {
    type Num = i32;
    type Items = Vec<i32>;
}

impl Action<csv_list::Items<Self>> for CsvActionsImpl {
    fn build(&mut self, node: csv_list::Items<Self>) -> Result<Vec<i32>, Self::Error> {
        let csv_list::Items::Items(nums) = node;
        Ok(nums)
    }
}

#[test]
fn test_separator_single() {
    let mut parser = csv_list::Parser::<CsvActionsImpl>::new();
    let mut actions = CsvActionsImpl;
    parser = parser
        .push(csv_list::Terminal::Num(42), &mut actions)
        .unwrap();
    let result = parser.finish(&mut actions).unwrap();
    assert_eq!(result, vec![42]);
}

#[test]
fn test_separator_multiple() {
    let mut parser = csv_list::Parser::<CsvActionsImpl>::new();
    let mut actions = CsvActionsImpl;
    parser = parser
        .push(csv_list::Terminal::Num(1), &mut actions)
        .unwrap();
    parser = parser
        .push(csv_list::Terminal::Comma, &mut actions)
        .unwrap();
    parser = parser
        .push(csv_list::Terminal::Num(2), &mut actions)
        .unwrap();
    parser = parser
        .push(csv_list::Terminal::Comma, &mut actions)
        .unwrap();
    parser = parser
        .push(csv_list::Terminal::Num(3), &mut actions)
        .unwrap();
    let result = parser.finish(&mut actions).unwrap();
    assert_eq!(result, vec![1, 2, 3]);
}

// Test passthrough (same non-terminal to same non-terminal)
gazelle! {
    grammar paren {
        start expr;
        terminals {
            NUM: _,
            LPAREN,
            RPAREN
        }

        expr = LPAREN expr RPAREN => paren
             | NUM => literal;
    }
}

struct ParenActionsImpl;

impl gazelle::ErrorType for ParenActionsImpl {
    type Error = core::convert::Infallible;
}

impl paren::Types for ParenActionsImpl {
    type Num = i32;
    type Expr = i32;
}

impl Action<paren::Expr<Self>> for ParenActionsImpl {
    fn build(&mut self, node: paren::Expr<Self>) -> Result<i32, Self::Error> {
        Ok(match node {
            paren::Expr::Paren(e) => e,
            paren::Expr::Literal(n) => n,
        })
    }
}

#[test]
fn test_passthrough() {
    let mut parser = paren::Parser::<ParenActionsImpl>::new();
    let mut actions = ParenActionsImpl;

    // Parse: (42)
    parser = parser.push(paren::Terminal::Lparen, &mut actions).unwrap();
    parser = parser.push(paren::Terminal::Num(42), &mut actions).unwrap();
    parser = parser.push(paren::Terminal::Rparen, &mut actions).unwrap();

    let result = parser.finish(&mut actions).unwrap();
    assert_eq!(result, 42);
}

// Test file include
gazelle! {
    grammar file_expr = "tests/test.gzl"
}

struct FileExprActionsImpl;

impl gazelle::ErrorType for FileExprActionsImpl {
    type Error = core::convert::Infallible;
}

impl file_expr::Types for FileExprActionsImpl {
    type Num = i32;
    type Expr = i32;
    type Term = i32;
}

impl Action<file_expr::Expr<Self>> for FileExprActionsImpl {
    fn build(&mut self, node: file_expr::Expr<Self>) -> Result<i32, Self::Error> {
        Ok(match node {
            file_expr::Expr::Add(left, right) => left + right,
            file_expr::Expr::TermToExpr(t) => t,
        })
    }
}

impl Action<file_expr::Term<Self>> for FileExprActionsImpl {
    fn build(&mut self, node: file_expr::Term<Self>) -> Result<i32, Self::Error> {
        let file_expr::Term::Literal(n) = node;
        Ok(n)
    }
}

#[test]
fn test_file_include() {
    let mut parser = file_expr::Parser::<FileExprActionsImpl>::new();
    let mut actions = FileExprActionsImpl;

    // Parse: 1 + 2
    parser = parser
        .push(file_expr::Terminal::Num(1), &mut actions)
        .unwrap();
    parser = parser
        .push(file_expr::Terminal::Plus, &mut actions)
        .unwrap();
    parser = parser
        .push(file_expr::Terminal::Num(2), &mut actions)
        .unwrap();

    let result = parser.finish(&mut actions).unwrap();
    assert_eq!(result, 3);
}

// Custom fold ON `*`: `item* as Items` lets the user pick the container, and a
// non-Vec/context-bearing fold is a custom `Action<SeqNode<..>>` on the `*` node
// — same dispatch as every reduction. The default would be `Vec`; here it folds
// straight into a running sum, with `&mut self` available.
gazelle! {
    grammar nums {
        start prog;
        terminals {
            NUM: _,
            SEMI
        }

        prog = item* as Items => prog;
        item = NUM SEMI => item;
    }
}

struct Sum(i32); // not Default+Extend — the fold is in the Action below

struct NumsActions {
    folds: usize, // &mut self context is available in the fold
}

impl gazelle::ErrorType for NumsActions {
    type Error = core::convert::Infallible;
}

impl nums::Types for NumsActions {
    type Num = i32;
    type Item = i32;
    type Items = Sum; // the knob: any container you want
    type Prog = i32;
}

// the custom fold on `item*`:
impl Action<nums::SeqNode<Sum, i32>> for NumsActions {
    fn build(&mut self, node: nums::SeqNode<Sum, i32>) -> Result<Sum, Self::Error> {
        self.folds += 1;
        Ok(match node {
            nums::SeqNode::Empty => Sum(0),
            nums::SeqNode::Append(Sum(acc), n) => Sum(acc + n),
        })
    }
}

impl Action<nums::Item<Self>> for NumsActions {
    fn build(&mut self, node: nums::Item<Self>) -> Result<i32, Self::Error> {
        let nums::Item::Item(n) = node;
        Ok(n)
    }
}

impl Action<nums::Prog<Self>> for NumsActions {
    fn build(&mut self, node: nums::Prog<Self>) -> Result<i32, Self::Error> {
        let nums::Prog::Prog(items) = node;
        Ok(items.0)
    }
}

#[test]
fn test_named_sequence_custom_fold() {
    let mut parser = nums::Parser::<NumsActions>::new();
    let mut actions = NumsActions { folds: 0 };
    for n in [10, 20, 30] {
        parser = parser.push(nums::Terminal::Num(n), &mut actions).unwrap();
        parser = parser.push(nums::Terminal::Semi, &mut actions).unwrap();
    }
    let result = parser.finish(&mut actions).unwrap();
    assert_eq!(result, 60); // folded straight into a sum on the `*`, never a Vec
    assert!(actions.folds >= 4); // Empty + 3 Appends, all through &mut self
}

// Custom fold: spell the repetition as an explicit rule instead of `NUM*`.
// `sum` becomes a normal node you implement `Action` for — the fold logic is
// yours (here a running sum, not even a container), with &mut self available.
gazelle! {
    grammar fold {
        start sum;
        terminals {
            NUM: _
        }

        sum = _ => zero
            | sum NUM => add;
    }
}

struct FoldActions {
    steps: usize, // &mut self context is available in the fold
}

impl gazelle::ErrorType for FoldActions {
    type Error = core::convert::Infallible;
}

impl fold::Types for FoldActions {
    type Num = i64;
    type Sum = i64; // the accumulator — anything you want, not a container
}

impl Action<fold::Sum<Self>> for FoldActions {
    fn build(&mut self, node: fold::Sum<Self>) -> Result<i64, Self::Error> {
        self.steps += 1;
        Ok(match node {
            fold::Sum::Zero => 0,
            fold::Sum::Add(acc, n) => acc + n,
        })
    }
}

#[test]
fn test_custom_fold() {
    let mut parser = fold::Parser::<FoldActions>::new();
    let mut actions = FoldActions { steps: 0 };
    for n in [10, 20, 30] {
        parser = parser.push(fold::Terminal::Num(n), &mut actions).unwrap();
    }
    let result = parser.finish(&mut actions).unwrap();
    assert_eq!(result, 60);
    assert!(actions.steps >= 4); // zero + 3 adds, all through &mut self
}

// Exercise the generated ManuallyDrop union at each ownership boundary. These
// tests detect selecting the wrong union member, leaking a shifted payload, or
// draining one twice.
gazelle! {
    grammar drop_union {
        start pair;
        terminals {
            ITEM: _
        }

        pair = ITEM ITEM => pair;
    }
}

#[derive(Debug)]
struct Tracked {
    id: usize,
    drops: std::rc::Rc<Vec<std::cell::Cell<usize>>>,
}

impl Tracked {
    fn new(id: usize, drops: &std::rc::Rc<Vec<std::cell::Cell<usize>>>) -> Self {
        Self {
            id,
            drops: drops.clone(),
        }
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        let count = self.drops[self.id].get();
        self.drops[self.id].set(count + 1);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ReductionFailed;

struct DropActions {
    fail: bool,
    panic_in_range: bool,
    panic_in_action: bool,
}

impl gazelle::ErrorType for DropActions {
    type Error = ReductionFailed;
}

impl drop_union::Types for DropActions {
    type Item = Tracked;
    type Pair = Vec<Tracked>;

    fn set_token_range(&mut self, _start: usize, _end: usize) {
        assert!(!self.panic_in_range, "panic from set_token_range");
    }
}

impl Action<drop_union::Pair<Self>> for DropActions {
    fn build(&mut self, node: drop_union::Pair<Self>) -> Result<Vec<Tracked>, Self::Error> {
        let drop_union::Pair::Pair(left, right) = node;
        assert!(!self.panic_in_action, "panic from Action::build");
        if self.fail {
            Err(ReductionFailed)
        } else {
            Ok(vec![left, right])
        }
    }
}

fn drop_counts(len: usize) -> std::rc::Rc<Vec<std::cell::Cell<usize>>> {
    std::rc::Rc::new((0..len).map(|_| std::cell::Cell::new(0)).collect())
}

fn observed_drops(drops: &std::rc::Rc<Vec<std::cell::Cell<usize>>>) -> Vec<usize> {
    drops.iter().map(std::cell::Cell::get).collect()
}

#[test]
fn semantic_union_drops_successful_result_once() {
    let drops = drop_counts(2);
    let mut actions = DropActions {
        fail: false,
        panic_in_range: false,
        panic_in_action: false,
    };
    let parser = drop_union::Parser::<DropActions>::new();
    let parser = parser
        .push(
            drop_union::Terminal::Item(Tracked::new(0, &drops)),
            &mut actions,
        )
        .unwrap();
    let parser = parser
        .push(
            drop_union::Terminal::Item(Tracked::new(1, &drops)),
            &mut actions,
        )
        .unwrap();

    let result = parser.finish(&mut actions).unwrap();
    assert_eq!(observed_drops(&drops), [0, 0]);
    drop(result);
    assert_eq!(observed_drops(&drops), [1, 1]);
}

#[test]
fn semantic_union_drops_stack_and_rejected_terminal_on_syntax_error() {
    let drops = drop_counts(3);
    let mut actions = DropActions {
        fail: false,
        panic_in_range: false,
        panic_in_action: false,
    };
    let parser = drop_union::Parser::<DropActions>::new();
    let parser = parser
        .push(
            drop_union::Terminal::Item(Tracked::new(0, &drops)),
            &mut actions,
        )
        .unwrap();
    let parser = parser
        .push(
            drop_union::Terminal::Item(Tracked::new(1, &drops)),
            &mut actions,
        )
        .unwrap();

    let error = match parser.push(
        drop_union::Terminal::Item(Tracked::new(2, &drops)),
        &mut actions,
    ) {
        Ok(_) => panic!("third item was unexpectedly accepted"),
        Err(error) => error,
    };

    assert!(matches!(&error, gazelle::ParseError::Syntax { .. }));
    assert_eq!(observed_drops(&drops), [1, 1, 1]);
    drop(error);
    assert_eq!(observed_drops(&drops), [1, 1, 1]);
}

#[test]
fn semantic_union_drops_reduction_inputs_once_on_action_error() {
    let drops = drop_counts(2);
    let mut actions = DropActions {
        fail: true,
        panic_in_range: false,
        panic_in_action: false,
    };
    let parser = drop_union::Parser::<DropActions>::new();
    let parser = parser
        .push(
            drop_union::Terminal::Item(Tracked::new(0, &drops)),
            &mut actions,
        )
        .unwrap();
    let parser = parser
        .push(
            drop_union::Terminal::Item(Tracked::new(1, &drops)),
            &mut actions,
        )
        .unwrap();

    let error = parser.finish(&mut actions).unwrap_err();
    assert!(matches!(
        &error,
        gazelle::ParseError::Action {
            error: ReductionFailed,
            ..
        }
    ));
    assert_eq!(observed_drops(&drops), [1, 1]);
    drop(error);
    assert_eq!(observed_drops(&drops), [1, 1]);
}

fn parser_with_two_tracked_items(
    drops: &std::rc::Rc<Vec<std::cell::Cell<usize>>>,
    actions: &mut DropActions,
) -> drop_union::Parser<DropActions> {
    let parser = drop_union::Parser::<DropActions>::new();
    let parser = parser
        .push(drop_union::Terminal::Item(Tracked::new(0, drops)), actions)
        .unwrap();
    parser
        .push(drop_union::Terminal::Item(Tracked::new(1, drops)), actions)
        .unwrap()
}

#[test]
fn semantic_union_is_unwind_safe_when_range_callback_panics() {
    let drops = drop_counts(2);
    let mut actions = DropActions {
        fail: false,
        panic_in_range: true,
        panic_in_action: false,
    };
    let parser = parser_with_two_tracked_items(&drops, &mut actions);

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = parser.finish(&mut actions);
    }));
    assert!(panic.is_err());
    assert_eq!(observed_drops(&drops), [1, 1]);
}

#[test]
fn semantic_union_is_unwind_safe_when_action_panics() {
    let drops = drop_counts(2);
    let mut actions = DropActions {
        fail: false,
        panic_in_range: false,
        panic_in_action: true,
    };
    let parser = parser_with_two_tracked_items(&drops, &mut actions);

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = parser.finish(&mut actions);
    }));
    assert!(panic.is_err());
    assert_eq!(observed_drops(&drops), [1, 1]);
}
