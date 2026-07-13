//! C11 Calculator - full C11 expression syntax with variables, function calls,
//! and user-defined operators.
//!
//! Variables live in a flat array; `&x` returns the slot index, `*n` dereferences slot n.
//! All assignment operators (=, +=, <<=, etc.) are BINOP variants.
//! Builtins: pow(a, b), min(a, b), max(a, b).
//! Custom operators: `operator @ pow right 3;` binds `@` to pow with right-assoc prec 3.
//! Statements separated by `;`, each pushes its result for testing.

use gazelle::Precedence;
use gazelle_macros::gazelle;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
enum BinOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    // Bitwise
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    // Logical
    And,
    Or,
    // Comparison
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    // Assignment
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
    ShlAssign,
    ShrAssign,
    BitAndAssign,
    BitOrAssign,
    BitXorAssign,
    // User-defined
    Custom(char),
}

gazelle! {
    grammar c11_calc {
        start stmts;
        terminals {
            NUM: _,
            IDENT: _,
            LPAREN, RPAREN, LBRACK, RBRACK,
            COMMA, COLON, SEMI,
            TILDE, BANG,
            INC, DEC,
            OPERATOR, LEFT, RIGHT,
            prec QUESTION,
            prec STAR,
            prec AMP,
            prec PLUS,
            prec MINUS,
            prec BINOP: _
        }

        stmts = stmts SEMI stmt => append | stmt => single | _ => empty;

        assoc = LEFT => left | RIGHT => right;
        stmt = OPERATOR BINOP IDENT assoc NUM => def_op
             | expression => print;

        primary_expression = NUM => num
                                 | IDENT => ident
                                 | LPAREN expression RPAREN => paren;

        postfix_expression = primary_expression => primary
                                 | postfix_expression LBRACK expression RBRACK => index
                                 | postfix_expression LPAREN RPAREN => call0
                                 | postfix_expression LPAREN argument_expression_list RPAREN => call
                                 | postfix_expression INC => postinc
                                 | postfix_expression DEC => postdec;

        argument_expression_list = assignment_expression => single
                                                         | argument_expression_list COMMA assignment_expression => append;

        unary_expression = postfix_expression => postfix
                               | INC unary_expression => preinc
                               | DEC unary_expression => predec
                               | AMP cast_expression => addr
                               | STAR cast_expression => deref
                               | PLUS cast_expression => uplus
                               | MINUS cast_expression => uminus
                               | TILDE cast_expression => bitnot
                               | BANG cast_expression => lognot;

        cast_expression = unary_expression => unary;

        binary_op = BINOP => binop
                         | STAR => mul
                         | AMP => bitand
                         | PLUS => add
                         | MINUS => sub;

        assignment_expression = cast_expression => cast
                                    | assignment_expression binary_op assignment_expression => binary
                                    | assignment_expression QUESTION expression COLON assignment_expression => ternary;

        expression = assignment_expression => assign
                         | expression COMMA assignment_expression => comma;
    }
}

/// Value: either an rvalue (plain integer) or an lvalue (slot index into vars).
#[derive(Clone, Copy, Debug)]
enum Val {
    Rval(i64),
    Lval(usize),
}

#[derive(Clone)]
struct OpDef {
    func: String,
    prec: Precedence,
}

struct Eval {
    vars: Vec<i64>,
    slot_names: Vec<String>,
    names: HashMap<String, usize>,
    custom_ops: HashMap<char, OpDef>,
    results: Vec<i64>,
}

impl Eval {
    fn new() -> Self {
        Self {
            vars: Vec::new(),
            slot_names: Vec::new(),
            names: HashMap::new(),
            custom_ops: HashMap::new(),
            results: Vec::new(),
        }
    }

    fn ensure_slot(&mut self, slot: usize) {
        if slot >= self.vars.len() {
            self.vars.resize(slot + 1, 0);
            self.slot_names.resize(slot + 1, String::new());
        }
    }

    fn slot(&mut self, name: &str) -> usize {
        if let Some(&s) = self.names.get(name) {
            return s;
        }
        let s = self.vars.len();
        self.vars.push(0);
        self.slot_names.push(name.to_string());
        self.names.insert(name.to_string(), s);
        s
    }

    fn get(&mut self, v: Val) -> i64 {
        match v {
            Val::Rval(n) => n,
            Val::Lval(slot) => {
                self.ensure_slot(slot);
                self.vars[slot]
            }
        }
    }

    fn store(&mut self, lhs: Val, val: i64) -> Val {
        match lhs {
            Val::Lval(slot) => {
                self.ensure_slot(slot);
                self.vars[slot] = val;
                Val::Lval(slot)
            }
            Val::Rval(_) => panic!("assignment to rvalue"),
        }
    }
}

fn builtin(name: &str, a: i64, b: i64) -> i64 {
    match name {
        "pow" => {
            if b < 0 {
                return 0;
            }
            let mut r = 1i64;
            for _ in 0..b {
                r = r.wrapping_mul(a);
            }
            r
        }
        "min" => a.min(b),
        "max" => a.max(b),
        _ => panic!("unknown builtin: {}", name),
    }
}

impl gazelle::ErrorType for Eval {
    type Error = core::convert::Infallible;
}

impl c11_calc::Types for Eval {
    type Num = i64;
    type Ident = String;
    type Binop = BinOp;
    type Assoc = fn(u8) -> Precedence;
    type Stmts = gazelle::Ignore;
    type Stmt = ();
    type PrimaryExpression = Val;
    type PostfixExpression = Val;
    type ArgumentExpressionList = Vec<Val>;
    type UnaryExpression = Val;
    type CastExpression = Val;
    type BinaryOp = BinOp;
    type AssignmentExpression = Val;
    type Expression = Val;
}

// Associativity
impl gazelle::Action<c11_calc::Assoc<Self>> for Eval {
    fn build(&mut self, node: c11_calc::Assoc<Self>) -> Result<fn(u8) -> Precedence, Self::Error> {
        Ok(match node {
            c11_calc::Assoc::Left => Precedence::Left,
            c11_calc::Assoc::Right => Precedence::Right,
        })
    }
}

// Statement (untyped NT with => name → output is ())
impl gazelle::Action<c11_calc::Stmt<Self>> for Eval {
    fn build(&mut self, node: c11_calc::Stmt<Self>) -> Result<(), Self::Error> {
        match node {
            c11_calc::Stmt::DefOp(op, func, assoc, prec) => {
                if let BinOp::Custom(ch) = op {
                    self.custom_ops.insert(
                        ch,
                        OpDef {
                            func,
                            prec: assoc(prec as u8),
                        },
                    );
                }
            }
            c11_calc::Stmt::Print(e) => {
                let v = self.get(e);
                self.results.push(v);
            }
        }
        Ok(())
    }
}

// Primary expression
impl gazelle::Action<c11_calc::PrimaryExpression<Self>> for Eval {
    fn build(&mut self, node: c11_calc::PrimaryExpression<Self>) -> Result<Val, Self::Error> {
        Ok(match node {
            c11_calc::PrimaryExpression::Num(n) => Val::Rval(n),
            c11_calc::PrimaryExpression::Ident(name) => Val::Lval(self.slot(&name)),
            c11_calc::PrimaryExpression::Paren(e) => e,
        })
    }
}

// Postfix expression
impl gazelle::Action<c11_calc::PostfixExpression<Self>> for Eval {
    fn build(&mut self, node: c11_calc::PostfixExpression<Self>) -> Result<Val, Self::Error> {
        Ok(match node {
            c11_calc::PostfixExpression::Primary(e) => e,
            c11_calc::PostfixExpression::Index(arr, idx) => {
                let base = self.get(arr) as usize;
                let i = self.get(idx) as usize;
                Val::Lval(base + i)
            }
            c11_calc::PostfixExpression::Call0(_func) => Val::Rval(0),
            c11_calc::PostfixExpression::Call(func, args) => {
                let name = match func {
                    Val::Lval(slot) => self.slot_names[slot].clone(),
                    Val::Rval(_) => panic!("call on rvalue"),
                };
                match args.len() {
                    2 => {
                        let a = self.get(args[0]);
                        let b = self.get(args[1]);
                        Val::Rval(builtin(&name, a, b))
                    }
                    _ => panic!("{}: expected 2 args, got {}", name, args.len()),
                }
            }
            c11_calc::PostfixExpression::Postinc(e) => {
                let v = self.get(e);
                if let Val::Lval(_) = e {
                    self.store(e, v + 1);
                }
                Val::Rval(v)
            }
            c11_calc::PostfixExpression::Postdec(e) => {
                let v = self.get(e);
                if let Val::Lval(_) = e {
                    self.store(e, v - 1);
                }
                Val::Rval(v)
            }
        })
    }
}

// Argument expression list
impl gazelle::Action<c11_calc::ArgumentExpressionList<Self>> for Eval {
    fn build(
        &mut self,
        node: c11_calc::ArgumentExpressionList<Self>,
    ) -> Result<Vec<Val>, Self::Error> {
        Ok(match node {
            c11_calc::ArgumentExpressionList::Single(e) => vec![e],
            c11_calc::ArgumentExpressionList::Append(mut list, e) => {
                list.push(e);
                list
            }
        })
    }
}

// Unary expression
impl gazelle::Action<c11_calc::UnaryExpression<Self>> for Eval {
    fn build(&mut self, node: c11_calc::UnaryExpression<Self>) -> Result<Val, Self::Error> {
        Ok(match node {
            c11_calc::UnaryExpression::Postfix(e) => e,
            c11_calc::UnaryExpression::Preinc(e) => {
                let v = self.get(e) + 1;
                self.store(e, v);
                Val::Rval(v)
            }
            c11_calc::UnaryExpression::Predec(e) => {
                let v = self.get(e) - 1;
                self.store(e, v);
                Val::Rval(v)
            }
            c11_calc::UnaryExpression::Addr(e) => match e {
                Val::Lval(slot) => Val::Rval(slot as i64),
                Val::Rval(_) => panic!("address of rvalue"),
            },
            c11_calc::UnaryExpression::Deref(e) => Val::Lval(self.get(e) as usize),
            c11_calc::UnaryExpression::Uplus(e) => Val::Rval(self.get(e)),
            c11_calc::UnaryExpression::Uminus(e) => Val::Rval(-self.get(e)),
            c11_calc::UnaryExpression::Bitnot(e) => Val::Rval(!self.get(e)),
            c11_calc::UnaryExpression::Lognot(e) => Val::Rval(if self.get(e) == 0 { 1 } else { 0 }),
        })
    }
}

// Cast expression (passthrough)
impl gazelle::Action<c11_calc::CastExpression<Self>> for Eval {
    fn build(&mut self, node: c11_calc::CastExpression<Self>) -> Result<Val, Self::Error> {
        let c11_calc::CastExpression::Unary(e) = node;
        Ok(e)
    }
}

// Binary op non-terminal
impl gazelle::Action<c11_calc::BinaryOp<Self>> for Eval {
    fn build(&mut self, node: c11_calc::BinaryOp<Self>) -> Result<BinOp, Self::Error> {
        Ok(match node {
            c11_calc::BinaryOp::Binop(op) => op,
            c11_calc::BinaryOp::Mul => BinOp::Mul,
            c11_calc::BinaryOp::Bitand => BinOp::BitAnd,
            c11_calc::BinaryOp::Add => BinOp::Add,
            c11_calc::BinaryOp::Sub => BinOp::Sub,
        })
    }
}

// Assignment expression (binary + ternary)
impl gazelle::Action<c11_calc::AssignmentExpression<Self>> for Eval {
    fn build(&mut self, node: c11_calc::AssignmentExpression<Self>) -> Result<Val, Self::Error> {
        Ok(match node {
            c11_calc::AssignmentExpression::Cast(e) => e,
            c11_calc::AssignmentExpression::Binary(l, op, r) => {
                match op {
                    // Assignment operators
                    BinOp::Assign => {
                        let v = self.get(r);
                        self.store(l, v)
                    }
                    BinOp::AddAssign => {
                        let v = self.get(l) + self.get(r);
                        self.store(l, v)
                    }
                    BinOp::SubAssign => {
                        let v = self.get(l) - self.get(r);
                        self.store(l, v)
                    }
                    BinOp::MulAssign => {
                        let v = self.get(l) * self.get(r);
                        self.store(l, v)
                    }
                    BinOp::DivAssign => {
                        let v = self.get(l) / self.get(r);
                        self.store(l, v)
                    }
                    BinOp::ModAssign => {
                        let v = self.get(l) % self.get(r);
                        self.store(l, v)
                    }
                    BinOp::ShlAssign => {
                        let v = self.get(l) << self.get(r);
                        self.store(l, v)
                    }
                    BinOp::ShrAssign => {
                        let v = self.get(l) >> self.get(r);
                        self.store(l, v)
                    }
                    BinOp::BitAndAssign => {
                        let v = self.get(l) & self.get(r);
                        self.store(l, v)
                    }
                    BinOp::BitOrAssign => {
                        let v = self.get(l) | self.get(r);
                        self.store(l, v)
                    }
                    BinOp::BitXorAssign => {
                        let v = self.get(l) ^ self.get(r);
                        self.store(l, v)
                    }
                    // User-defined operator
                    BinOp::Custom(ch) => {
                        let func = self
                            .custom_ops
                            .get(&ch)
                            .unwrap_or_else(|| panic!("undefined operator: {}", ch))
                            .func
                            .clone();
                        let a = self.get(l);
                        let b = self.get(r);
                        Val::Rval(builtin(&func, a, b))
                    }
                    // Arithmetic / comparison / logic
                    _ => {
                        let lv = self.get(l);
                        let rv = self.get(r);
                        Val::Rval(match op {
                            BinOp::Add => lv + rv,
                            BinOp::Sub => lv - rv,
                            BinOp::Mul => lv * rv,
                            BinOp::Div => lv / rv,
                            BinOp::Mod => lv % rv,
                            BinOp::BitAnd => lv & rv,
                            BinOp::BitOr => lv | rv,
                            BinOp::BitXor => lv ^ rv,
                            BinOp::Shl => lv << rv,
                            BinOp::Shr => lv >> rv,
                            BinOp::And => {
                                if lv != 0 && rv != 0 {
                                    1
                                } else {
                                    0
                                }
                            }
                            BinOp::Or => {
                                if lv != 0 || rv != 0 {
                                    1
                                } else {
                                    0
                                }
                            }
                            BinOp::Eq => {
                                if lv == rv {
                                    1
                                } else {
                                    0
                                }
                            }
                            BinOp::Ne => {
                                if lv != rv {
                                    1
                                } else {
                                    0
                                }
                            }
                            BinOp::Lt => {
                                if lv < rv {
                                    1
                                } else {
                                    0
                                }
                            }
                            BinOp::Gt => {
                                if lv > rv {
                                    1
                                } else {
                                    0
                                }
                            }
                            BinOp::Le => {
                                if lv <= rv {
                                    1
                                } else {
                                    0
                                }
                            }
                            BinOp::Ge => {
                                if lv >= rv {
                                    1
                                } else {
                                    0
                                }
                            }
                            _ => unreachable!(),
                        })
                    }
                }
            }
            c11_calc::AssignmentExpression::Ternary(cond, then_val, else_val) => {
                if self.get(cond) != 0 {
                    then_val
                } else {
                    else_val
                }
            }
        })
    }
}

// Expression (comma)
impl gazelle::Action<c11_calc::Expression<Self>> for Eval {
    fn build(&mut self, node: c11_calc::Expression<Self>) -> Result<Val, Self::Error> {
        Ok(match node {
            c11_calc::Expression::Assign(e) => e,
            c11_calc::Expression::Comma(_l, r) => r,
        })
    }
}

// =============================================================================
// Tokenizer (wraps gazelle::lexer::Scanner over any char iterator)
// =============================================================================

struct Tokenizer<I: Iterator<Item = char>> {
    src: gazelle::lexer::Scanner<I>,
}

#[cfg(test)]
impl<'a> Tokenizer<std::str::Chars<'a>> {
    fn from_str(input: &'a str) -> Self {
        Self {
            src: gazelle::lexer::Scanner::new(input),
        }
    }
}

impl<I: Iterator<Item = char>> Tokenizer<I> {
    fn from_chars(iter: I) -> Self {
        Self {
            src: gazelle::lexer::Scanner::from_chars(iter),
        }
    }

    fn next(
        &mut self,
        custom_ops: &HashMap<char, OpDef>,
    ) -> Result<Option<c11_calc::Terminal<Eval>>, String> {
        self.src.skip_whitespace();
        while self.src.skip_line_comment("//") || self.src.skip_block_comment("/*", "*/") {
            self.src.skip_whitespace();
        }

        if self.src.at_end() {
            return Ok(None);
        }

        // Number
        if self.src.peek().is_some_and(|c| c.is_ascii_digit()) {
            let mut s = String::new();
            while let Some(c) = self.src.peek() {
                if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '_' {
                    s.push(c);
                    self.src.advance();
                } else {
                    break;
                }
            }
            // Remove underscores for parsing
            s.retain(|c| c != '_');
            return Ok(Some(c11_calc::Terminal::Num(s.parse().unwrap_or(0))));
        }

        // Identifier or keyword
        if self
            .src
            .peek()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        {
            let mut s = String::new();
            while let Some(c) = self.src.peek() {
                if c.is_alphanumeric() || c == '_' {
                    s.push(c);
                    self.src.advance();
                } else {
                    break;
                }
            }
            return Ok(Some(match s.as_str() {
                "operator" => c11_calc::Terminal::Operator,
                "left" => c11_calc::Terminal::Left,
                "right" => c11_calc::Terminal::Right,
                _ => c11_calc::Terminal::Ident(s),
            }));
        }

        // Punctuation
        if let Some(c) = self.src.peek() {
            match c {
                '(' => {
                    self.src.advance();
                    return Ok(Some(c11_calc::Terminal::Lparen));
                }
                ')' => {
                    self.src.advance();
                    return Ok(Some(c11_calc::Terminal::Rparen));
                }
                '[' => {
                    self.src.advance();
                    return Ok(Some(c11_calc::Terminal::Lbrack));
                }
                ']' => {
                    self.src.advance();
                    return Ok(Some(c11_calc::Terminal::Rbrack));
                }
                ',' => {
                    self.src.advance();
                    return Ok(Some(c11_calc::Terminal::Comma));
                }
                ';' => {
                    self.src.advance();
                    return Ok(Some(c11_calc::Terminal::Semi));
                }
                _ => {}
            }
        }

        // Multi-char operators (longest first for maximal munch)
        const MULTI_OPS: &[&str] = &[
            "<<=", ">>=", // 0-1: three-char
            "++", "--", "<<", ">>", "<=", ">=", "==", "!=", // 2-9
            "&&", "||", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", // 10-19
        ];
        const MULTI_TERMINALS: &[fn() -> c11_calc::Terminal<Eval>] = &[
            || c11_calc::Terminal::Binop(BinOp::ShlAssign, Precedence::Right(1)),
            || c11_calc::Terminal::Binop(BinOp::ShrAssign, Precedence::Right(1)),
            || c11_calc::Terminal::Inc,
            || c11_calc::Terminal::Dec,
            || c11_calc::Terminal::Binop(BinOp::Shl, Precedence::Left(10)),
            || c11_calc::Terminal::Binop(BinOp::Shr, Precedence::Left(10)),
            || c11_calc::Terminal::Binop(BinOp::Le, Precedence::Left(9)),
            || c11_calc::Terminal::Binop(BinOp::Ge, Precedence::Left(9)),
            || c11_calc::Terminal::Binop(BinOp::Eq, Precedence::Left(8)),
            || c11_calc::Terminal::Binop(BinOp::Ne, Precedence::Left(8)),
            || c11_calc::Terminal::Binop(BinOp::And, Precedence::Left(4)),
            || c11_calc::Terminal::Binop(BinOp::Or, Precedence::Left(3)),
            || c11_calc::Terminal::Binop(BinOp::AddAssign, Precedence::Right(1)),
            || c11_calc::Terminal::Binop(BinOp::SubAssign, Precedence::Right(1)),
            || c11_calc::Terminal::Binop(BinOp::MulAssign, Precedence::Right(1)),
            || c11_calc::Terminal::Binop(BinOp::DivAssign, Precedence::Right(1)),
            || c11_calc::Terminal::Binop(BinOp::ModAssign, Precedence::Right(1)),
            || c11_calc::Terminal::Binop(BinOp::BitAndAssign, Precedence::Right(1)),
            || c11_calc::Terminal::Binop(BinOp::BitOrAssign, Precedence::Right(1)),
            || c11_calc::Terminal::Binop(BinOp::BitXorAssign, Precedence::Right(1)),
        ];

        if let Some((idx, _)) = self.src.read_one_of(MULTI_OPS) {
            return Ok(Some(MULTI_TERMINALS[idx]()));
        }

        // Single-char operators
        if let Some(c) = self.src.peek() {
            self.src.advance();
            return Ok(Some(match c {
                '+' => c11_calc::Terminal::Plus(Precedence::Left(11)),
                '-' => c11_calc::Terminal::Minus(Precedence::Left(11)),
                '*' => c11_calc::Terminal::Star(Precedence::Left(12)),
                '&' => c11_calc::Terminal::Amp(Precedence::Left(7)),
                '~' => c11_calc::Terminal::Tilde,
                '!' => c11_calc::Terminal::Bang,
                ':' => c11_calc::Terminal::Colon,
                '?' => c11_calc::Terminal::Question(Precedence::Right(2)),
                '/' => c11_calc::Terminal::Binop(BinOp::Div, Precedence::Left(12)),
                '%' => c11_calc::Terminal::Binop(BinOp::Mod, Precedence::Left(12)),
                '<' => c11_calc::Terminal::Binop(BinOp::Lt, Precedence::Left(9)),
                '>' => c11_calc::Terminal::Binop(BinOp::Gt, Precedence::Left(9)),
                '^' => c11_calc::Terminal::Binop(BinOp::BitXor, Precedence::Left(6)),
                '|' => c11_calc::Terminal::Binop(BinOp::BitOr, Precedence::Left(5)),
                '=' => c11_calc::Terminal::Binop(BinOp::Assign, Precedence::Right(1)),
                // Custom operator
                ch => {
                    let prec = custom_ops
                        .get(&ch)
                        .map(|d| d.prec)
                        .unwrap_or(Precedence::Left(0));
                    c11_calc::Terminal::Binop(BinOp::Custom(ch), prec)
                }
            }));
        }

        Err("Unexpected end of input".to_string())
    }
}

fn run<I: Iterator<Item = char>>(tokenizer: &mut Tokenizer<I>) -> Result<Vec<i64>, String> {
    let mut parser = c11_calc::Parser::<Eval>::new();
    let mut actions = Eval::new();

    while let Some(tok) = tokenizer.next(&actions.custom_ops)? {
        parser = parser
            .push(tok, &mut actions)
            .map_err(|e| format!("{:?}", e))?;
    }
    parser
        .finish(&mut actions)
        .map_err(|gazelle::ParseError::Syntax { terminal, recovery }| {
            recovery.format_error(terminal, None, None)
        })?;

    Ok(actions.results)
}

fn main() {
    use std::io::Read;
    let mut tokenizer =
        Tokenizer::from_chars(std::io::stdin().lock().bytes().map(|b| b.unwrap() as char));
    match run(&mut tokenizer) {
        Ok(results) => {
            for r in &results {
                println!("{}", r);
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c11_calculator() {
        let input = "
            // Arithmetic
            1 + 2 * 3;
            2 * 3 + 1;
            (1 + 2) * 3;
            10 - 3 - 2;
            100 / 10 / 2;
            10 % 3;

            // All precedence levels
            1 || 0;
            0 && 1;
            5 | 2;
            7 ^ 3;
            7 & 3;
            2 == 2;
            2 != 3;
            1 < 2;
            2 <= 2;
            3 > 2;
            3 >= 3;
            1 << 3;
            8 >> 2;

            // Mixed precedence
            1 + 2 * 3 + 4;
            1 || 0 && 0;
            1 + 1 == 2;
            2 * 3 == 6;
            1 < 2 == 1;
            1 + 2 < 4;

            // Unary
            -5;
            +5;
            !0;
            !1;
            ~0;

            // Ternary
            1 ? 2 : 3;
            0 ? 2 : 3;
            1 ? 2 : 0 ? 3 : 4;
            0 ? 2 : 1 ? 3 : 4;
            1 + 1 ? 2 : 3;

            // Comma
            1, 2, 3;
            1 + 2, 3 + 4;

            // Postfix on rvalue
            5++;
            5--;

            // Assignment
            x = 10; x;
            x = 3; y = 4; x + y;
            x = 0; y = 0; x = y = 42; x;

            // Compound assignment
            a = 10; a += 5; a;
            b = 10; b -= 3; b;
            c = 10; c *= 2; c;
            d = 20; d /= 4; d;
            e = 10; e %= 3; e;
            f = 1; f <<= 3; f;
            g = 8; g >>= 2; g;
            h = 7; h &= 3; h;
            i = 5; i |= 2; i;
            j = 7; j ^= 3; j;

            // Ref/deref, inc/dec
            k = 42; *&k;
            l = 5; ++l; l;
            m = 5; --m; m;
            n = 5; n++; n;
            o = 5; o--; o;

            // Complex sequences
            p = 1; q = 2; r = p + q * 3; r;
            s = 10; s += 5; s *= 2; s;
            t = 0; u = 1; t ? 10 : u ? 20 : 30;

            // Builtin functions
            pow(2, 10);
            min(3, 7);
            max(3, 7);
            pow(2, 3) + 1;
            min(1 + 2, 4);

            // Custom operators
            operator @ pow right 13; 2 @ 10;
            operator @ pow right 13; 2 @ 3 @ 2;
            operator @ pow right 13; 1 + 2 @ 3;
            operator @ max left 13;
            operator # min left 13;
            3 @ 5 # 4
        ";
        assert_eq!(
            run(&mut Tokenizer::from_str(input)).unwrap(),
            vec![
                // Arithmetic
                7, 7, 9, 5, 5, 1, // All precedence levels
                1, 0, 7, 4, 3, 1, 1, 1, 1, 1, 1, 8, 2, // Mixed precedence
                11, 1, 1, 1, 1, 1, // Unary
                -5, 5, 1, 0, -1, // Ternary
                2, 3, 2, 3, 2, // Comma
                3, 7, // Postfix on rvalue
                5, 5, // Assignment
                10, 10, 3, 4, 7, 0, 0, 42, 42, // Compound assignment
                10, 15, 15, 10, 7, 7, 10, 20, 20, 20, 5, 5, 10, 1, 1, 1, 8, 8, 8, 2, 2, 7, 3, 3, 5,
                7, 7, 7, 4, 4, // Ref/deref, inc/dec
                42, 42, 5, 6, 6, 5, 4, 4, 5, 5, 6, 5, 5, 4, // Complex sequences
                1, 2, 7, 7, 10, 15, 30, 30, 0, 1, 20, // Builtins
                1024, 3, 7, 9, 3, // Custom operators
                1024, 512, 9, 4,
            ]
        );
    }
}
