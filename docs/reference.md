# Gazelle Reference

Complete reference for the Gazelle parser generator.

## Project Status

**Working and tested:**
- Grammar definition (`.gzl` files and `gazelle!` macro)
- Minimal LR table generation
- Type-safe parser generation with `Types`/`Action` traits
- Precedence terminals (`prec`) for runtime operator precedence
- Terminal modifiers for S/R conflict resolution (`shift`, `reduce`, `conflict`)
- Modifiers: `?` (optional), `*` (zero+), `+` (one+), `%` (separated list)
- Expected conflict declarations (`expect N rr/sr`)
- Conflict diagnostics with concrete example inputs showing both parses
- Token range tracking for source spans
- Push-based parsing (you control the loop)
- Detailed error messages with parser state context
- Automatic error recovery (Dijkstra-based minimum-cost repair)

**Tested on:**
- Calculator with user-defined operators
- C11 parser (complete grammar, 41 tests)

## Table of Contents

- [How It Works](#how-it-works)
- [Grammar Syntax](#grammar-syntax)
- [The gazelle! Macro](#the-grammar-macro)
- [Generated Types](#generated-types)
- [Using the Parser](#using-the-parser)
- [Advanced Features](#advanced-features)

---

## How It Works

Most parser generators either embed semantic actions in the grammar (yacc-style `$$` / `$1`) or produce a generic, type-erased concrete syntax tree you have to walk later. Gazelle does neither. It produces an abstract syntax tree — abstract because it's not presented as a data structure, but as a **pattern of calls**.

The AST nodes are enum variants that arise naturally from the grammar — one enum per non-terminal, one variant per alternative. During parsing, every time a rule is reduced, Gazelle calls your `Action::build` with a node containing the children's already-reduced values. This sequence of calls *is* a post-order traversal of the parse tree — the tree is abstractly present without ever being materialized.

You provide the (possibly stateful) mapping from each node to its output via the `Action` trait. A `Types` trait declares the output type per symbol. What you return is up to you: a computed value, a tree node, or nothing at all.

### Direct evaluation

Since reductions see already-reduced children, you can fold the tree into values as it's parsed — no intermediate tree:

```rust
impl calc::Types for Evaluator {
    type Error = ParseError;
    type Num = f64;
    type Op = char;
    type Expr = f64;  // expressions reduce to numbers
}

impl gazelle::Action<calc::Expr<Self>> for Evaluator {
    fn build(&mut self, node: calc::Expr<Self>) -> Result<f64, ParseError> {
        // node.0 is f64, node.1 is char, node.2 is f64
        Ok(match node {
            calc::Expr::Binop(l, op, r) => match op {
                '+' => l + r, '*' => l * r, _ => 0.0,
            },
            calc::Expr::Literal(n) => n,
        })
    }
}
```

### Materializing a tree

If you *do* want a tree, set associated types to the node enums themselves. Each reduction stores its node, and the tree materializes through the normal reduction flow — no custom `Action` impl needed:

```rust
impl calc::Types for CstBuilder {
    type Error = ParseError;
    type Num = f64;
    type Op = char;
    type Expr = Box<calc::Expr<Self>>;  // recursive, needs Box
}
// No Action impl — blanket handles it
```

Gazelle supports a few blanket reductions that allow you to generate a CST or just validate syntax without writing `Action` impls:
- **Identity**: `type Expr = calc::Expr<Self>` — node passes through unchanged (CST)
- **Box**: `type Expr = Box<calc::Expr<Self>>` — node is auto-boxed (CST with recursive types)
- **Ignore**: `type Expr = Ignore` — node is discarded (validation only)

You only write a custom `Action` impl when you need custom logic.

### Why Box is needed for recursive types

The generated enum for a recursive rule like `expr = expr OP expr => binop | NUM => literal` contains itself:

```rust
pub enum Expr<A: Types> {
    Binop(A::Expr, A::Op, A::Expr),  // contains A::Expr
    Literal(A::Num),
}
```

If `type Expr = calc::Expr<Self>`, the type is infinitely sized — `Expr` contains `Expr` contains `Expr`... Rust won't allow this. Wrapping in `Box` breaks the cycle: `type Expr = Box<calc::Expr<Self>>` gives the compiler a known size (one pointer). The auto-box blanket handles the wrapping automatically.

Non-recursive types (like a `Statement` that contains an `Expr` but not another `Statement`) don't need boxing and can use identity directly.

### Mix and match

You can mix strategies in one implementation — evaluate some nodes, build trees for others, ignore the rest:

```rust
impl my_grammar::Types for MyActions {
    type Expr = i64;                           // evaluate
    type Statement = Box<my_grammar::Statement<Self>>; // build tree (boxed)
    type Comment = Ignore;                     // discard
    // ...
}
// Only need Action for Expr (custom logic)
// Statement and Comment are handled by blankets
```

---

## Grammar Syntax

Gazelle grammars can be written in `.gzl` files or inline with the `gazelle!` macro.

### Basic Structure

`.gzl` files contain the grammar directly:

```
start rule_name;

terminals {
    // terminal declarations
}

// rule definitions
```

In the `gazelle!` macro, the grammar is wrapped with `grammar Name { ... }`:

```rust
gazelle! {
    grammar Name {
        start rule_name;
        terminals { ... }
        // rule definitions
    }
}
```

### Terminal Declarations

Terminals are tokens from your lexer. Declare them in the `terminals` block:

```
terminals {
    // Simple terminal (no payload, becomes () in generated code)
    LPAREN,
    RPAREN,

    // Terminal with payload — `: _` generates an associated type
    // named after the terminal (NUM → type Num, IDENT → type Ident)
    NUM: _,
    IDENT: _,

    // Precedence terminal (for operator precedence parsing)
    prec OP: _,         // Carries precedence at runtime
    prec BINOP,         // Prec terminal without payload

    // Conflict resolution modifiers
    shift ELSE,         // Always shift on S/R conflict (e.g. dangling else)
    reduce SEMI,        // Always reduce on S/R conflict
    conflict TOK: _,    // Caller decides per-token (via Resolution)
}
```

**Terminal modifiers** control how shift/reduce conflicts are resolved:

| Modifier | Behavior | Use case |
|----------|----------|----------|
| *(none)* | S/R conflict is an error | Default — grammar must be unambiguous |
| `prec` | Resolved by runtime precedence (`Precedence::Left`/`Right`) | Expression operators |
| `shift` | Always shift | Dangling else, longest match |
| `reduce` | Always reduce | Rare; when shorter parse is preferred |
| `conflict` | Caller supplies `Resolution` per token | Mixed strategies at runtime |

Modifiers work by preventing S/R conflicts from appearing in the parser tables entirely. The table construction phase emits the correct action (shift, reduce, or shift-or-reduce) based on the modifier, so no runtime overhead for `shift`/`reduce` — the decision is baked into the table.

**Terminal naming convention:** Terminals should be UPPERCASE to distinguish from non-terminals.

### Rule Definitions

Rules define the grammar's structure. Every alternative requires `=> name`:

```
// Single alternative
expr = expr PLUS term => add;

// Multiple alternatives (separated by |)
expr = expr PLUS term => add
     | expr MINUS term => sub
     | term => term;
```

### Actions and Enum Generation

Each `=> name` generates an enum variant. Untyped symbols (no `: Type`) are omitted from variant fields:

```
expr = expr PLUS term => add;
// Generates enum variant: Expr::Add(A::Expr, A::Term)
// PLUS is untyped, so it's omitted — only typed symbols become fields
```

**Untyped rules** - if the non-terminal has no type annotation, output is `()`. The `Action` is still called for side effects:

```
// statement has no type annotation — output is ()
statement = expr SEMI => on_statement;
// Generates: Statement::OnStatement(A::Expr)
// Action<Statement<Self>>::build returns Result<(), Error>
```

### Modifiers

**Optional** (`?`) - zero or one:
```
trailing_comma = COMMA?;
// Generates Option<T> where T is the symbol's type
```

**Repetition** (`*`) - zero or more:
```
args = arg*;
// Generates Vec<T>
```

**One or more** (`+`) - at least one:
```
statements = statement+;
// Generates Vec<T>
```

**Separated list** (`%`) - one or more separated by a delimiter:
```
args = expr % COMMA;
// Generates Vec<T> where T is expr's type
// Parses: expr, expr COMMA expr, expr COMMA expr COMMA expr, ...
```

### Expect Declarations

Declare expected conflicts to suppress errors:

```
start translation_unit;
expect 3 rr;  // Expect 3 reduce/reduce conflicts
expect 1 sr;  // Expect 1 shift/reduce conflict

// ... rest of grammar
```

Use this for grammars with known ambiguities (like C's typedef). The count must match exactly — if the grammar changes and the conflict count shifts, the build fails.

**Prefer terminal modifiers over `expect` for S/R conflicts.** For example, the classic dangling-else conflict is better handled with `shift ELSE` than `expect 1 sr`, because the modifier makes the intent explicit and eliminates the conflict at the source rather than suppressing the diagnostic.

---

## The gazelle! Macro

The `gazelle!` macro generates a type-safe parser at compile time.

### Basic Usage

```rust
use gazelle_macros::gazelle;
use gazelle::Precedence;

gazelle! {
    grammar Calc {
        start expr;

        terminals {
            NUM: _,
            LPAREN, RPAREN,
            prec OP: _,
        }

        expr = expr OP expr => binop
             | NUM => literal
             | LPAREN expr RPAREN => paren;
    }
}
```

### Visibility

Add `pub` before `grammar` for public visibility:

```rust
gazelle! {
    pub grammar MyParser {
        // ...
    }
}
```

Or with restricted visibility:

```rust
gazelle! {
    pub(crate) grammar MyParser {
        // ...
    }
}
```

---

## Generated Types

The macro generates a module (snake_case of grammar name, e.g., `calc`) containing:

### Types Trait

`Types` declares associated types and the error type:

```rust
pub trait Types: Sized {
    type Error: From<ParseError>;
    type Num: Debug;
    type Op: Debug;
    type Expr: Debug;
    fn set_token_range(&mut self, start: usize, end: usize) {}
}
```

### Per-Node Enums

Each non-terminal with named alternatives generates an enum generic over `A: Types`:

```rust
pub enum Expr<A: Types> {
    Binop(A::Expr, A::Op, A::Expr),
    Literal(A::Num),
}

impl<A: Types> AstNode for Expr<A> {
    type Output = A::Expr;
    type Error = A::Error;
}
```

A blanket `Action` impl handles identity (CST), `Box<N>` (auto-boxing), and `Ignore` (discard) automatically. You only write a custom `Action` impl when you need custom logic.

### Terminal Enum

Represents input tokens:

```rust
pub enum Terminal<A: Types> {
    Num(A::Num),
    Lparen,
    Rparen,
    Op(A::Op, Precedence),  // prec terminals include Precedence
}
```

### Parser Struct

```rust
pub struct Parser<A: Types> { /* ... */ }

impl<A: Types + Action<Expr<A>>> Parser<A> {
    pub fn new() -> Self;
    pub fn push(&mut self, terminal: Terminal<A>, actions: &mut A) -> Result<(), A::Error>;
    pub fn finish(self, actions: &mut A) -> Result<A::Expr, (Self, A::Error)>;
    pub fn state(&self) -> usize;
    pub fn format_error(&self, err: &ParseError) -> String;
}
```

Note: `finish` returns `(Self, A::Error)` on error, giving back the parser so you can still call `format_error`.

---

## Using the Parser

### Step 1: Implement Types and Actions

```rust
use gazelle::{ParseError, Action};

struct Evaluator;

impl calc::Types for Evaluator {
    type Error = ParseError;
    type Num = f64;
    type Op = char;
    type Expr = f64;
}

impl Action<calc::Expr<Self>> for Evaluator {
    fn build(&mut self, node: calc::Expr<Self>) -> Result<f64, ParseError> {
        Ok(match node {
            calc::Expr::Binop(left, op, right) => match op {
                '+' => left + right,
                '-' => left - right,
                '*' => left * right,
                '/' => left / right,
                _ => panic!("unknown operator"),
            },
            calc::Expr::Literal(n) => n,
        })
    }
}
```

### Step 2: Create Parser and Push Tokens

```rust
use gazelle::Precedence;

fn parse(input: &str) -> Result<f64, String> {
    let mut parser = calc::Parser::<Evaluator>::new();
    let mut actions = Evaluator;

    // Your lexer loop
    for token in lex(input) {
        let terminal = match token {
            Token::Num(n) => calc::Terminal::Num(n),
            Token::Plus => calc::Terminal::Op('+', Precedence::Left(1)),
            Token::Star => calc::Terminal::Op('*', Precedence::Left(2)),
            Token::LParen => calc::Terminal::Lparen,
            Token::RParen => calc::Terminal::Rparen,
        };

        parser.push(terminal, &mut actions)
            .map_err(|e| parser.format_error(&e))?;
    }

    parser.finish(&mut actions)
        .map_err(|(p, e)| p.format_error(&e))
}
```

### Step 3: Handle Errors

```rust
// Push errors - parser is still available for format_error
match parser.push(terminal, &mut actions) {
    Ok(()) => { /* continue */ }
    Err(e) => {
        let msg = parser.format_error(&e);
        // msg contains: "unexpected 'X', expected: A, B, C"
        //               "  after: tokens parsed so far"
        //               "  in rule: context"
        return Err(msg);
    }
}

// Finish errors - parser returned in the error tuple
match parser.finish(&mut actions) {
    Ok(result) => { /* use result */ }
    Err((parser, e)) => {
        let msg = parser.format_error(&e);
        return Err(msg);
    }
}
```

---

## Advanced Features

### Precedence Terminals

For expression grammars, `prec` terminals carry precedence at runtime instead of encoding it in grammar structure:

```rust
terminals {
    prec OP: _,
}

expr = expr OP expr => binop | atom => atom;
```

When lexing, attach precedence to each operator:

```rust
'+' => calc::Terminal::Op('+', Precedence::Left(1)),   // Lower precedence
'*' => calc::Terminal::Op('*', Precedence::Left(2)),   // Higher precedence
'^' => calc::Terminal::Op('^', Precedence::Right(3)),  // Right-associative
```

**Precedence values:**
- `Precedence::Left(n)` - left-associative with level n
- `Precedence::Right(n)` - right-associative with level n
- Higher n = tighter binding

This enables user-defined operators at runtime!

### Token Range Tracking (Spans)

Implement `set_token_range` to track source positions:

```rust
impl calc::Types for SpanTracker {
    type Error = ParseError;
    type Op = char;
    type Num = f64;
    type Expr = Expr;

    fn set_token_range(&mut self, start: usize, end: usize) {
        // Called before each reduction with [start, end) token indices
        self.current_span = self.token_spans[start].start..self.token_spans[end-1].end;
    }
}

impl Action<calc::Expr<Self>> for SpanTracker {
    fn build(&mut self, node: calc::Expr<Self>) -> Result<Expr, ParseError> {
        Ok(match node {
            calc::Expr::Binop(left, op, right) => Expr {
                kind: ExprKind::BinOp(Box::new(left), op, Box::new(right)),
                span: self.current_span.clone(),
            },
            // ...
        })
    }
}
```

### Lexer Feedback

For languages like C where lexing depends on parse state (typedef disambiguation):

```rust
struct CActions {
    typedefs: HashSet<String>,
}

impl Action<c11::DeclTypedef<Self>> for CActions {
    fn build(&mut self, node: c11::DeclTypedef<Self>) -> Result<...> {
        // Extract name, register it
        self.typedefs.insert(name.clone());
        // ...
    }
}

// In your parse loop:
loop {
    // Lexer sees current typedef set
    let token = lexer.next(&actions.typedefs)?;
    parser.push(token, &mut actions)?;
}
```

The push-based architecture makes this natural - you control the loop.

### Multiple Implementations

The same grammar can have multiple action implementations:

```rust
// Evaluator
impl calc::Types for Evaluator { type Expr = f64; /* ... */ }
impl Action<calc::Expr<Self>> for Evaluator {
    fn build(&mut self, node: calc::Expr<Self>) -> Result<f64, ParseError> { /* evaluate */ }
}

// CST Builder — Box needed because Expr is recursive
// The blanket Action handles it automatically — no impl needed!
impl calc::Types for CstBuilder { type Expr = Box<calc::Expr<Self>>; /* ... */ }

// Pretty Printer
impl calc::Types for Printer { type Expr = String; /* ... */ }
impl Action<calc::Expr<Self>> for Printer {
    fn build(&mut self, node: calc::Expr<Self>) -> Result<String, ParseError> { /* format */ }
}
```

### Runtime Grammar API

For dynamic grammars, use the library API directly:

```rust
use gazelle::{parse_grammar, GrammarBuilder};
use gazelle::table::CompiledTable;
use gazelle::runtime::{CstParser, Token};

// Parse from string
let grammar = parse_grammar(r#"
    start expr;
    terminals { NUM, PLUS }
    expr = expr PLUS expr => add | NUM => num;
"#)?;

// Or build programmatically
let mut gb = GrammarBuilder::new();
let num = gb.t("NUM");
let plus = gb.t("PLUS");
let expr = gb.nt("expr");
gb.rule(expr, vec![expr, plus, expr]);
gb.rule(expr, vec![num]);
let grammar = gb.build();

// Compile and parse
let compiled = CompiledTable::build(&grammar).unwrap();
let mut parser = CstParser::new(compiled.table());

let num_id = compiled.symbol_id("NUM").unwrap();
let plus_id = compiled.symbol_id("PLUS").unwrap();

parser.push(Token::new(num_id))?;     // NUM
parser.push(Token::new(plus_id))?;    // PLUS
parser.push(Token::new(num_id))?;     // NUM

let tree = parser.finish().map_err(|(p, e)| p.format_error(&e, &compiled))?;
// tree is a Cst: Leaf nodes carry SymbolId + token index,
// Node carry rule index + children
```

`CstParser` mirrors the generated parser's `push`/`finish` pattern. `Cst::Leaf` includes a token index so you can map back to your own token data (values, source positions, etc.).

For building a custom AST instead of a CST, use `Parser` directly with `maybe_reduce`/`shift`:

```rust
use gazelle::runtime::{Parser, Token};

let compiled = CompiledTable::build(&grammar).unwrap();
let mut parser = Parser::new(compiled.table());
let mut stack: Vec<MyAst> = Vec::new();
let mut iter = tokens.into_iter();

loop {
    let token = iter.next();
    // Reduce while possible (rule 0 = accept)
    while let Some((rule, len, _)) = parser.maybe_reduce(token)? {
        if rule == 0 { return stack.pop().unwrap(); }
        let children: Vec<MyAst> = stack.drain(stack.len() - len..).collect();
        stack.push(build_ast_node(rule, children));
    }
    let Some(token) = token else { unreachable!() };
    stack.push(MyAst::Leaf(token));
    parser.shift(token);
}
```

---

## Errors and Recovery

### Parse errors

When `push` or `finish` returns an error, `format_error` produces a message showing what went wrong, what was expected, and where in the grammar the parser was:

```
unexpected 'STAR', expected: NUM, LPAREN
  after: expr PLUS
  in expr: expr OP • expr
```

The `•` marks the parser's position in the rule — it had seen `expr OP` and expected the right-hand operand.

Errors from `push` leave the parser intact so you can still call `format_error`:

```rust
parser.push(terminal, &mut actions).map_err(|e| parser.format_error(&e))?;
```

Errors from `finish` return the parser in the error tuple for the same reason:

```rust
parser.finish(&mut actions).map_err(|(p, e)| p.format_error(&e))?;
```

For nicer output, `format_error_with` lets you provide display names (mapping internal symbol names to user-facing ones) and the actual token texts for the "after:" context line:

```rust
let display_names = HashMap::from([("PLUS", "+"), ("STAR", "*"), ("LPAREN", "(")]);
let token_texts = vec!["1", "+", "*"];  // the tokens parsed so far
let msg = parser.format_error_with(&e, &display_names, &token_texts);
// unexpected '*', expected: NUM, (
//   after: 1 +
//   in expr: expr OP • expr
```

### Error recovery

When a parse error occurs, you can call `recover` on the low-level `Parser` to find a minimum-cost repair and continue parsing. Recovery uses Dijkstra search over possible insert/delete edits to find the cheapest way to get the parser back on track.

```rust
use gazelle::runtime::{Parser, Token, Repair};

// Parse tokens, recover on error
let mut pos = 0;
while pos < tokens.len() {
    loop {
        match parser.maybe_reduce(Some(tokens[pos])) {
            Ok(None) => break,              // ready to shift
            Ok(Some((0, _, _))) => return,  // accept
            Ok(Some(_)) => continue,        // reduce, keep going
            Err(_) => {
                // Error — recover with remaining tokens
                let errors = parser.recover(&tokens[pos..]);
                for e in &errors {
                    let repairs: Vec<_> = e.repairs.iter().map(|r| match r {
                        Repair::Insert(id) => format!("insert '{}'", ctx.symbol_name(*id)),
                        Repair::Delete(id) => format!("delete '{}'", ctx.symbol_name(*id)),
                        Repair::Shift => "shift".to_string(),
                    }).collect();
                    eprintln!("error at token {}: {}", e.position, repairs.join(", "));
                }
                return;
            }
        }
    }
    parser.shift(tokens[pos]);
    pos += 1;
}
```

Each `RecoveryInfo` contains a position (token index where the error was detected) and a list of `Repair` actions:
- `Repair::Insert(id)` — a missing token was inserted (e.g., a forgotten `;`)
- `Repair::Delete(id)` — an extra token was deleted (e.g., a stray `+`)
- `Repair::Shift` — a token was shifted normally (free cost, not a real edit)

Recovery can find multiple errors in one pass — it repairs and continues until the end of input.

### Conflict diagnostics

During table generation, Gazelle reports shift/reduce and reduce/reduce conflicts with concrete examples showing both parses:

```
Shift/reduce conflict on 'ELSE':
  Shift:  stmt -> IF COND THEN stmt • ELSE stmt (wins)
  Reduce: stmt -> IF COND THEN stmt •
  Example: IF COND THEN IF COND THEN stmt • ELSE stmt
  Shift:  IF COND THEN IF COND THEN (stmt ELSE stmt)
  Reduce: IF COND THEN (IF COND THEN stmt) ELSE stmt (reduce to stmt)
```

Each conflict shows:
- The two competing items with dot positions (`•`)
- Which action wins (shift wins for S/R, first rule wins for R/R)
- A concrete example input with a dot at the conflict point
- Two bracketings showing how each parse would group the input

When no single input string works for both parses (the grammar is LR(k>1) rather than ambiguous), separate examples are shown for each interpretation:

```
Shift/reduce conflict on 'C':
  Shift:  x -> A • C (wins)
  Reduce: x -> A •
  Shift example:  A • C B
    (A C B)
  Reduce example: A • C D
    (A) C D (reduce to x)
```

For S/R conflicts, prefer terminal modifiers (`shift`, `reduce`, `conflict`, `prec`) which resolve the conflict at the source. Use `expect N rr;` / `expect N sr;` to suppress remaining conflicts when the count matches (like C's typedef ambiguity). Unmatched counts still produce errors.
