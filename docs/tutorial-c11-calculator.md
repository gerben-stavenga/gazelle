# Building a Calculator: From Simple to C11

This tutorial builds a calculator step by step, starting simple and adding features until we have full C11 expression syntax with user-defined operators. Each step introduces a Gazelle feature to solve a real problem.

## Step 1: A Basic Calculator

Let's start with arithmetic: `+`, `-`, `*`, `/`. We need precedence — `*` binds tighter than `+` — so we use the traditional cascading grammar:

```rust
use gazelle_macros::gazelle;

gazelle! {
    grammar Calc {
        start expr;
        terminals {
            NUM: _,
            PLUS, MINUS, STAR, SLASH,
            LPAREN, RPAREN,
        }

        expr = add_expr => add_expr;

        add_expr = add_expr PLUS mul_expr => add
                 | add_expr MINUS mul_expr => sub
                 | mul_expr => mul_expr;

        mul_expr = mul_expr STAR primary => mul
                 | mul_expr SLASH primary => div
                 | primary => primary;

        primary = NUM => num
                | LPAREN expr RPAREN => paren;
    }
}
```

This works. `1 + 2 * 3` parses as `1 + (2 * 3)` because `mul_expr` is lower in the cascade than `add_expr`.

Every alternative has `=> name`, which generates an enum variant. Untyped terminals (like `LPAREN`, `RPAREN`, `PLUS`) are omitted from variant fields — only typed symbols become fields.

### What Gets Generated

The `gazelle!` macro generates a `calc` module containing several things. First, a `Types` trait and per-node enums:

```rust
trait Types: ErrorType + Sized {
    type Num;
    type AddExpr;
    type MulExpr;
    type Primary;
}

enum AddExpr<A: Types> {
    Add(A::AddExpr, A::MulExpr),
    Sub(A::AddExpr, A::MulExpr),
}

enum MulExpr<A: Types> { Mul(A::MulExpr, A::Primary), Div(A::MulExpr, A::Primary) }
enum Primary<A: Types> { Num(A::Num), Paren(A::Expr) }
```

An `Actions` trait is auto-implemented for any type satisfying `Types` + all `Action` bounds. You only write `Action` impls for nodes with custom logic — identity (CST), `Box<N>` (auto-boxing), and `Ignore` (discard) are handled by blanket impls.

Second, a terminal enum generic over Types:

```rust
enum Terminal<A: Types> {
    Num(A::Num),
    Plus, Minus, Star, Slash, Lparen, Rparen,
}
```

Third, a parser struct `Parser<A: Types>` with `push` and `finish` methods.

### Implementing the Traits

```rust
use gazelle::{ParseError, Action};

struct Eval;

impl calc::Types for Eval {
    type Error = ParseError;
    type Num = i64;
    type AddExpr = i64;
    type MulExpr = i64;
    type Primary = i64;
}

impl Action<calc::AddExpr<Self>> for Eval {
    fn build(&mut self, node: calc::AddExpr<Self>) -> Result<i64, ParseError> {
        Ok(match node {
            calc::AddExpr::Add(l, r) => l + r,
            calc::AddExpr::Sub(l, r) => l - r,
        })
    }
}

impl Action<calc::MulExpr<Self>> for Eval {
    fn build(&mut self, node: calc::MulExpr<Self>) -> Result<i64, ParseError> {
        Ok(match node {
            calc::MulExpr::Mul(l, r) => l * r,
            calc::MulExpr::Div(l, r) => l / r,
        })
    }
}

impl Action<calc::Primary<Self>> for Eval {
    fn build(&mut self, node: calc::Primary<Self>) -> Result<i64, ParseError> {
        Ok(match node {
            calc::Primary::Num(n) => n,
            calc::Primary::Paren(e) => e,
        })
    }
}
```

One `Action` impl per non-terminal enum.

### The Lexer

Gazelle provides a `Scanner` type with composable methods for building lexers. We use its methods to read tokens and map them to our terminal enum:

```rust
use gazelle::lexer::Scanner;

fn tokenize(input: &str) -> Result<Vec<calc::Terminal<Eval>>, String> {
    let mut src = Scanner::new(input);
    let mut tokens = Vec::new();

    loop {
        src.skip_whitespace();
        if src.at_end() { break; }

        if let Some(span) = src.read_digits() {
            let s = &input[span];
            tokens.push(calc::Terminal::Num(s.parse().unwrap()));
        } else if let Some(c) = src.peek() {
            src.advance();
            tokens.push(match c {
                '(' => calc::Terminal::Lparen,
                ')' => calc::Terminal::Rparen,
                '+' => calc::Terminal::Plus,
                '-' => calc::Terminal::Minus,
                '*' => calc::Terminal::Star,
                '/' => calc::Terminal::Slash,
                _ => return Err(format!("unexpected char: {}", c)),
            });
        }
    }
    Ok(tokens)
}
```

`Scanner` provides methods like `read_digits()`, `read_ident()`, and
`skip_whitespace()`. The read methods return byte ranges you can use to extract
text from the input.

### Running the Parser

```rust
fn run(input: &str) -> Result<i64, String> {
    let tokens = tokenize(input)?;
    let mut parser = calc::Parser::<Eval>::new();
    let mut actions = Eval;

    for token in tokens {
        parser = parser.push(token, &mut actions).map_err(|error| {
            calc::format_error(&error, None, None)
                .unwrap_or_else(|| "semantic action failed".to_string())
        })?;
    }
    parser.finish(&mut actions).map_err(|error| {
        calc::format_error(&error, None, None)
            .unwrap_or_else(|| "semantic action failed".to_string())
    })
}

fn main() {
    let result = run("1 + 2 * 3").unwrap();
    println!("{}", result);  // prints: 7
}
```

The parser is push-based: you feed tokens one at a time. Each `push` may trigger reductions, calling methods on your `actions` impl. When input is exhausted, `finish` returns the final value — the result of reducing the start symbol (`expr`).

## Step 2: Multiple Statements

Let's allow multiple expressions separated by semicolons.

```rust
gazelle! {
    grammar Calc {
        start stmts;
        terminals {
            NUM: _,
            PLUS, MINUS, STAR, SLASH,
            LPAREN, RPAREN,
            SEMI,
        }

        stmts = stmt*;
        stmt = add_expr SEMI => print;

        add_expr = add_expr PLUS mul_expr => add
                 | add_expr MINUS mul_expr => sub
                 | mul_expr => mul_expr;

        mul_expr = mul_expr STAR primary => mul
                 | mul_expr SLASH primary => div
                 | primary => primary;

        primary = NUM => num
                | LPAREN add_expr RPAREN => paren;
    }
}
```

Gazelle supports modifiers on symbols: `*` (zero or more), `+` (one or more), `?` (optional). Here `stmt*` means zero or more statements. Each `stmt` prints an expression and expects a semicolon.

Since `stmts` is untyped, `finish` returns `Result<(), _>`. The `print` action triggers an `Action::build` call:

```rust
struct Eval;

impl calc::Types for Eval {
    type Error = ParseError;
    type Num = i64;
    type AddExpr = i64;
    type MulExpr = i64;
    type Primary = i64;
}

impl Action<calc::Stmt<Self>> for Eval {
    fn build(&mut self, node: calc::Stmt<Self>) -> Result<(), ParseError> {
        match node {
            calc::Stmt::Print(e) => { println!("{}", e); Ok(()) }
        }
    }
}

impl Action<calc::AddExpr<Self>> for Eval {
    fn build(&mut self, node: calc::AddExpr<Self>) -> Result<i64, ParseError> {
        Ok(match node {
            calc::AddExpr::Add(l, r) => l + r,
            calc::AddExpr::Sub(l, r) => l - r,
        })
    }
}

// Similar Action impls for MulExpr, Primary...

fn run(input: &str) -> Result<(), String> {
    let tokens = tokenize(input)?;
    let mut parser = calc::Parser::<Eval>::new();
    let mut actions = Eval;

    for token in tokens {
        parser = parser.push(token, &mut actions).map_err(|error| {
            calc::format_error(&error, None, None)
                .unwrap_or_else(|| "semantic action failed".to_string())
        })?;
    }
    parser.finish(&mut actions).map_err(|error| {
        calc::format_error(&error, None, None)
            .unwrap_or_else(|| "semantic action failed".to_string())
    })
}
```

Now users can type several calculations:

```
> 1 + 2; 3 * 4; 5;
3
12
5
```

## Step 3: The Cascade Problem

Our grammar handles 2 precedence levels with 2 non-terminals. C11 has 15 levels. That means 15 non-terminals:

```
assignment_expr → conditional_expr → logical_or_expr → logical_and_expr →
bitwise_or_expr → bitwise_xor_expr → bitwise_and_expr → equality_expr →
relational_expr → shift_expr → additive_expr → multiplicative_expr →
cast_expr → unary_expr → postfix_expr → primary_expr
```

Each level has the same pattern: `this_level OP next_level`. Every rule produces the same semantic result — a value. The repetition is painful and obscures the actual structure: binary operation, two operands, one operator.

## Step 4: Runtime Precedence

Gazelle solves this with precedence terminals. Mark operators with `prec` and attach precedence values in the lexer:

```rust
gazelle! {
    grammar Calc {
        start stmts;
        terminals {
            NUM: _,
            LPAREN, RPAREN,
            SEMI,
            prec BINOP: _,
        }

        stmts = stmts SEMI expr => stmt
              | expr => first
              | _;

        expr = expr BINOP expr => binary
             | primary => primary;

        primary = NUM => num
                | LPAREN expr RPAREN => paren;
    }
}
```

One rule for all binary expressions. The lexer provides precedence:

```rust
fn tokenize(op: &str) -> calc::Terminal<Eval> {
    match op {
        "+" => calc::Terminal::Binop(BinOp::Add, Precedence::Left(11)),
        "-" => calc::Terminal::Binop(BinOp::Sub, Precedence::Left(11)),
        "*" => calc::Terminal::Binop(BinOp::Mul, Precedence::Left(12)),
        "/" => calc::Terminal::Binop(BinOp::Div, Precedence::Left(12)),
        // ...
    }
}
```

Higher numbers bind tighter. `Precedence::Left` for left-associative, `Precedence::Right` for right-associative.

The traits simplify:

```rust
impl calc::Types for Eval {
    type Error = ParseError;
    type Num = i64;
    type Binop = BinOp;
    type Expr = i64;
}

impl Action<calc::Expr<Self>> for Eval {
    fn build(&mut self, node: calc::Expr<Self>) -> Result<i64, ParseError> {
        Ok(match node {
            calc::Expr::Binary(l, op, r) => match op {
                BinOp::Add => l + r,
                BinOp::Sub => l - r,
                BinOp::Mul => l * r,
                BinOp::Div => l / r,
            },
            calc::Expr::Num(n) => n,
            calc::Expr::Paren(e) => e,
        })
    }
}
```

Now we can add all of C's binary operators by extending the `BinOp` enum and the `tokenize` function. The grammar doesn't change.

## Step 5: Variables

Let's add variables. We need to distinguish lvalues (assignable locations) from rvalues (plain values):

```rust
enum Val {
    Rval(i64),
    Lval(usize),  // index into variable storage
}
```

Update the grammar:

```rust
primary = NUM => num
        | IDENT => var
        | LPAREN expr RPAREN => paren;
```

And add assignment to the operators:

```rust
"=" => calc::Terminal::Binop(BinOp::Assign, Precedence::Right(1)),
```

Assignment is right-associative (`x = y = 5` assigns right-to-left) and lowest precedence.

```rust
impl Action<calc::Expr<Self>> for Eval {
    fn build(&mut self, node: calc::Expr<Self>) -> Result<Val, ParseError> {
        Ok(match node {
            calc::Expr::Binary(l, op, r) => match op {
                BinOp::Assign => {
                    let v = self.get(r);
                    self.store(l, v)
                }
                BinOp::Add => Val::Rval(self.get(l) + self.get(r)),
                // ...
            },
            calc::Expr::Num(n) => Val::Rval(n),
            calc::Expr::Var(name) => self.lookup(&name),
        })
    }
}
```

Now: `x = 10; y = 20; x + y` → `30`

## Step 6: Unary Operators

C has unary `+`, `-`, `!`, `~`, and the dual-role `*` (dereference) and `&` (address-of). Here's the problem: `*` and `&` are also binary operators (multiply and bitwise AND).

If `STAR` is a `prec BINOP`, we can't use it in unary rules:

```rust
unary_expr = STAR expr => deref   // won't work - STAR is BINOP
```

Solution: declare them as separate precedence terminals:

```rust
terminals {
    // ...
    prec STAR,
    prec AMP,
    prec PLUS,
    prec MINUS,
    prec BINOP: _,
}
```

Now we can use them in unary rules:

```rust
unary_expr = STAR unary_expr => deref
           | AMP unary_expr => addr
           | PLUS unary_expr => uplus
           | MINUS unary_expr => uminus
           | BANG unary_expr => lognot
           | TILDE unary_expr => bitnot
           | postfix_expr => postfix_expr;
```

But wait — now binary expressions don't see `STAR` as an operator. We need to collect all binary operators into one place:

```rust
binary_op = BINOP => binop   // BINOP already has the right type
          | STAR => op_mul
          | AMP => op_bitand
          | PLUS => op_add
          | MINUS => op_sub;

expr = expr binary_op expr => binary
     | unary_expr => unary_expr;
```

When `STAR` (precedence 12) reduces to `binary_op`, the non-terminal inherits that precedence. The parser resolves `1 + 2 * 3` correctly — the `binary_op` carrying `STAR`'s precedence wins over `PLUS`.

All alternatives need `=> name`. The `Action` for `BinaryOp` maps each variant to a `Binop` value.

## Step 7: Postfix Expressions

Function calls, array indexing, post-increment/decrement:

```rust
postfix_expr = primary => primary
             | postfix_expr LPAREN RPAREN => call0
             | postfix_expr LPAREN args RPAREN => call
             | postfix_expr LBRACK expr RBRACK => index
             | postfix_expr INC => postinc
             | postfix_expr DEC => postdec;

args = expr => arg1
     | args COMMA expr => arg;
```

Implementation handles function calls — we'll support builtins like `pow(2, 10)`:

```rust
impl Action<calc::PostfixExpr<Self>> for Eval {
    fn build(&mut self, node: calc::PostfixExpr<Self>) -> Result<Val, ParseError> {
        Ok(match node {
            calc::PostfixExpr::Call(func, args) => {
                let name = self.slot_name(func);
                match name.as_str() {
                    "pow" => {
                        let base = self.get(args[0]);
                        let exp = self.get(args[1]);
                        Val::Rval(base.pow(exp as u32))
                    }
                    "min" => Val::Rval(self.get(args[0]).min(self.get(args[1]))),
                    "max" => Val::Rval(self.get(args[0]).max(self.get(args[1]))),
                    _ => panic!("unknown function: {}", name),
                }
            }
            // ...
        })
    }
}
```

## Step 8: User-Defined Operators

The payoff. Let users define new operators:

```
operator @ pow right 13
2 @ 3 @ 2
```

This defines `@` as a right-associative operator at precedence 13, bound to the `pow` function. Then `2 @ 3 @ 2` computes `2^(3^2) = 2^9 = 512`.

Add statements for operator definition:

```rust
terminals {
    // ...
    LEFT, RIGHT,
}

assoc = LEFT => left | RIGHT => right;

stmt = OPERATOR BINOP IDENT assoc NUM => def_op
     | add_expr SEMI => print;
```

`LEFT` and `RIGHT` are unit terminals — no payload. The `Action` impls return the precedence constructor:

```rust
type Assoc = fn(u8) -> Precedence;

impl Action<calc::Assoc<Self>> for Eval {
    fn build(&mut self, node: calc::Assoc<Self>) -> Result<fn(u8) -> Precedence, ParseError> {
        Ok(match node {
            calc::Assoc::Left => Precedence::Left,
            calc::Assoc::Right => Precedence::Right,
        })
    }
}

impl Action<calc::Stmt<Self>> for Eval {
    fn build(&mut self, node: calc::Stmt<Self>) -> Result<(), ParseError> {
        match node {
            calc::Stmt::DefOp(op, func, assoc, prec) => {
                if let BinOp::Custom(ch) = op {
                    self.custom_ops.insert(ch, OpDef { func, prec: assoc(prec as u8) });
                }
                Ok(())
            }
            calc::Stmt::Print(e) => { println!("{}", e); Ok(()) }
        }
    }
}
```

The terminals distinguish left from right. The actions provide the behavior.

This shows the power of trait-based semantics: `type Assoc = fn(u8) -> Precedence` uses a function type as the associated type. You're not limited to simple data — any Rust type works.

Now the lexer needs to see this table. This is **lexer feedback** — information flowing from parser back to lexer.

The parse loop makes it natural:

```rust
let mut parser = calc::Parser::<Eval>::new();
let mut actions = Eval::new();

loop {
    // Lexer sees the current custom_ops table
    match tokenizer.next(&actions.custom_ops)? {
        Some(tok) => parser = parser.push(tok, &mut actions)?,
        None => break,
    }
}
```

The lexer consults `custom_ops` to get precedence for unknown single-character operators:

```rust
fn next(&mut self, custom_ops: &HashMap<char, OpDef>) -> Option<Token> {
    // ...
    if s.len() == 1 {
        let ch = s.chars().next().unwrap();
        let prec = custom_ops.get(&ch)
            .map(|d| d.prec)
            .unwrap_or(Precedence::Left(0));
        return Some(Token::Binop(BinOp::Custom(ch), prec));
    }
}
```

An unknown operator gets precedence 0 (lowest) until defined. Once defined, subsequent uses get the registered precedence.

## The Complete Picture

We've built:

1. **Basic arithmetic** with cascading grammar
2. **Statements** for interactive use
3. **Runtime precedence** to collapse the cascade
4. **Variables** with lvalue/rvalue distinction
5. **Unary operators** including dual-role `*` and `&`
6. **Precedence-carrying non-terminals** to unify binary operators
7. **Postfix expressions** for calls and indexing
8. **User-defined operators** with lexer feedback

The final grammar is ~40 lines. It handles the full C11 expression syntax plus user-defined operators. The grammar is clean because precedence lives in tokens, not grammar structure. Lexer feedback works because you control the parse loop.

See `examples/c11_calculator.rs` for the complete implementation.
