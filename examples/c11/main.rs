//! C11 Parser POC for Gazelle
//!
//! Demonstrates two key innovations:
//! 1. Jourdan's typedef disambiguation via NAME TYPE/NAME VARIABLE lexer feedback
//! 2. Dynamic precedence parsing via `prec` terminals - collapses 10 expression levels into one rule

use std::collections::HashSet;

use gazelle::Precedence;
use gazelle_macros::gazelle;

// =============================================================================
// Grammar Definition
// =============================================================================

gazelle! {
    #[derive(Debug)]
    grammar c11 = "grammars/c11.gzl"
}

// =============================================================================
// Placeholder types (no AST for now, just validate parsing)
// =============================================================================

// =============================================================================
// Typedef Context (Jourdan's approach: flat set with save/restore)
// =============================================================================

/// A context snapshot - the set of typedef names visible at a point
pub type Context = HashSet<String>;

/// Extract the declared name from a declarator CST.
fn declarator_name(d: &c11::Declarator<CActions>) -> &str {
    match d {
        c11::Declarator::DeclDirect(dd) | c11::Declarator::DeclPtr(_, dd) => {
            direct_declarator_name(dd)
        }
    }
}

fn direct_declarator_name(dd: &c11::DirectDeclarator<CActions>) -> &str {
    match dd {
        c11::DirectDeclarator::DdIdent(name) => name,
        c11::DirectDeclarator::DdParen(_, d) => declarator_name(d),
        c11::DirectDeclarator::DdOther(dd, ..)
        | c11::DirectDeclarator::DdOther1(dd, ..)
        | c11::DirectDeclarator::DdOther2(dd, ..)
        | c11::DirectDeclarator::DdOther3(dd, ..)
        | c11::DirectDeclarator::DdFunc(dd, _)
        | c11::DirectDeclarator::DdOtherKr(dd, ..) => direct_declarator_name(dd),
    }
}

/// Take the parameter context from the innermost DdFunc, if any.
fn declarator_param_ctx_take(d: &mut c11::Declarator<CActions>) -> Option<Context> {
    match d {
        c11::Declarator::DeclDirect(dd) | c11::Declarator::DeclPtr(_, dd) => dd_param_ctx_take(dd),
    }
}

fn dd_param_ctx_take(dd: &mut c11::DirectDeclarator<CActions>) -> Option<Context> {
    match dd {
        c11::DirectDeclarator::DdIdent(_) => None,
        c11::DirectDeclarator::DdParen(_, d) => declarator_param_ctx_take(d),
        // Prefer innermost DdFunc (closest to identifier)
        c11::DirectDeclarator::DdFunc(inner, Node(scoped)) => {
            dd_param_ctx_take(inner).or_else(|| {
                let c11::ScopedParameterTypeList::ScopedParams(_, ref mut ptl) = *scoped;
                let c11::ParameterTypeList::ParamCtx(_, _, ref mut ctx) = *ptl;
                Some(std::mem::take(ctx))
            })
        }
        c11::DirectDeclarator::DdOther(dd, ..)
        | c11::DirectDeclarator::DdOther1(dd, ..)
        | c11::DirectDeclarator::DdOther2(dd, ..)
        | c11::DirectDeclarator::DdOther3(dd, ..)
        | c11::DirectDeclarator::DdOtherKr(dd, ..) => dd_param_ctx_take(dd),
    }
}

/// Typedef context for tracking declared typedef names.
/// Uses Jourdan's approach: a single mutable set with save/restore.
#[derive(Debug)]
pub struct TypedefContext {
    current: HashSet<String>,
}

impl TypedefContext {
    pub fn new() -> Self {
        Self {
            current: HashSet::new(),
        }
    }

    /// Test if name is a typedef
    pub fn is_typedef(&self, name: &str) -> bool {
        self.current.contains(name)
    }

    /// Declare a typedef name (adds to set)
    pub fn declare_typedef(&mut self, name: &str) {
        self.current.insert(name.to_string());
    }

    /// Declare a variable name (removes from set, shadowing any typedef)
    pub fn declare_varname(&mut self, name: &str) {
        self.current.remove(name);
    }

    /// Save the current context (returns a snapshot)
    pub fn save(&self) -> Context {
        self.current.clone()
    }

    /// Restore a saved context
    pub fn restore(&mut self, snapshot: Context) {
        self.current = snapshot;
    }
}

impl Default for TypedefContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Newtype: wraps a CST node that needs side effects in its Reduce impl.
/// Avoids conflicting with the identity blanket `Reduce<N, N, E>`.
#[derive(Debug)]
pub struct Node<T: std::fmt::Debug>(T);

/// Actions for the C11 parser
#[derive(Debug)]
pub struct CActions {
    pub ctx: TypedefContext,
}

impl CActions {
    pub fn new() -> Self {
        Self {
            ctx: TypedefContext::new(),
        }
    }
}

impl Default for CActions {
    fn default() -> Self {
        Self::new()
    }
}

impl gazelle::ErrorType for CActions {
    type Error = core::convert::Infallible;
}

impl c11::Types for CActions {
    type Name = String;
    type Constant = String;
    type StringLiteral = String;
    type Binop = Precedence;
    type BinaryOp = c11::BinaryOp<Self>;
    type TypedefName = String;
    type VarName = String;
    type GeneralIdentifier = String;
    type EnumerationConstant = String;
    type SaveContext = Context;
    type ScopedCompoundStatement = Node<c11::ScopedCompoundStatement<Self>>;
    type ScopedIterationStatement = Node<c11::ScopedIterationStatement<Self>>;
    type ScopedParameterTypeList = Node<c11::ScopedParameterTypeList<Self>>;
    type ScopedSelectionStatement = Node<c11::ScopedSelectionStatement<Self>>;
    type ScopedStatement = Node<c11::ScopedStatement<Self>>;
    type DeclaratorVarname = Node<c11::DeclaratorVarname<Self>>;
    type DeclaratorTypedefname = Node<c11::DeclaratorTypedefname<Self>>;
    type Enumerator = Node<c11::Enumerator<Self>>;
    type FunctionDefinition1 = (Context, c11::FunctionDefinition1<Self>);
    type FunctionDefinition = Node<c11::FunctionDefinition<Self>>;
    // List types (all self-recursive → Box)
    type ListAnonymous0 = Box<c11::ListAnonymous0<Self>>;
    type ListAnonymous1 = Box<c11::ListAnonymous1<Self>>;
    type ListDeclarationSpecifier = Box<c11::ListDeclarationSpecifier<Self>>;
    type ListEq1TypedefDeclarationSpecifier = Box<c11::ListEq1TypedefDeclarationSpecifier<Self>>;
    type ListEq1TypeSpecifierUniqueAnonymous0 =
        Box<c11::ListEq1TypeSpecifierUniqueAnonymous0<Self>>;
    type ListEq1TypeSpecifierUniqueDeclarationSpecifier =
        Box<c11::ListEq1TypeSpecifierUniqueDeclarationSpecifier<Self>>;
    type ListGe1TypeSpecifierNonuniqueAnonymous1 =
        Box<c11::ListGe1TypeSpecifierNonuniqueAnonymous1<Self>>;
    type ListGe1TypeSpecifierNonuniqueDeclarationSpecifier =
        Box<c11::ListGe1TypeSpecifierNonuniqueDeclarationSpecifier<Self>>;
    type ListEq1Eq1TypedefTypeSpecifierUniqueDeclarationSpecifier =
        Box<c11::ListEq1Eq1TypedefTypeSpecifierUniqueDeclarationSpecifier<Self>>;
    type ListEq1Ge1TypedefTypeSpecifierNonuniqueDeclarationSpecifier =
        Box<c11::ListEq1Ge1TypedefTypeSpecifierNonuniqueDeclarationSpecifier<Self>>;
    // Leaf/non-recursive types
    type Declarator = c11::Declarator<Self>;
    type ParameterTypeList = c11::ParameterTypeList<Self>;
    type Variadic = c11::Variadic<Self>;
    type TypedefNameSpec = c11::TypedefNameSpec<Self>;
    type PrimaryExpression = c11::PrimaryExpression<Self>;
    type GenericSelection = c11::GenericSelection<Self>;
    type GenericAssociation = c11::GenericAssociation<Self>;
    type UnaryOperator = c11::UnaryOperator<Self>;
    type ConstantExpression = c11::ConstantExpression<Self>;
    type Declaration = c11::Declaration<Self>;
    type DeclarationSpecifier = c11::DeclarationSpecifier<Self>;
    type DeclarationSpecifiers = c11::DeclarationSpecifiers<Self>;
    type DeclarationSpecifiersTypedef = c11::DeclarationSpecifiersTypedef<Self>;
    type InitDeclaratorDeclaratorTypedefname = c11::InitDeclaratorDeclaratorTypedefname<Self>;
    type InitDeclaratorDeclaratorVarname = c11::InitDeclaratorDeclaratorVarname<Self>;
    type StorageClassSpecifier = c11::StorageClassSpecifier<Self>;
    type TypeSpecifierNonunique = c11::TypeSpecifierNonunique<Self>;
    type TypeSpecifierUnique = c11::TypeSpecifierUnique<Self>;
    type StructOrUnionSpecifier = c11::StructOrUnionSpecifier<Self>;
    type StructOrUnion = c11::StructOrUnion<Self>;
    type StructDeclaration = c11::StructDeclaration<Self>;
    type SpecifierQualifierList = c11::SpecifierQualifierList<Self>;
    type StructDeclarator = c11::StructDeclarator<Self>;
    type EnumSpecifier = c11::EnumSpecifier<Self>;
    type AtomicTypeSpecifier = c11::AtomicTypeSpecifier<Self>;
    type TypeQualifier = c11::TypeQualifier<Self>;
    type FunctionSpecifier = c11::FunctionSpecifier<Self>;
    type AlignmentSpecifier = c11::AlignmentSpecifier<Self>;
    type ParameterDeclaration = c11::ParameterDeclaration<Self>;
    type TypeName = c11::TypeName<Self>;
    type AbstractDeclarator = c11::AbstractDeclarator<Self>;
    type CInitializer = c11::CInitializer<Self>;
    type Designation = c11::Designation<Self>;
    type Designator = c11::Designator<Self>;
    type StaticAssertDeclaration = c11::StaticAssertDeclaration<Self>;
    type LabeledStatement = c11::LabeledStatement<Self>;
    type CompoundStatement = c11::CompoundStatement<Self>;
    type BlockItem = c11::BlockItem<Self>;
    type ExpressionStatement = c11::ExpressionStatement<Self>;
    type SelectionStatement = c11::SelectionStatement<Self>;
    type IterationStatement = c11::IterationStatement<Self>;
    type JumpStatement = c11::JumpStatement<Self>;
    type ExternalDeclaration = c11::ExternalDeclaration<Self>;
    // Self-recursive or mutually-recursive types (→ Box)
    type DirectDeclarator = Box<c11::DirectDeclarator<Self>>;
    type PostfixExpression = Box<c11::PostfixExpression<Self>>;
    type ArgumentExpressionList = c11::ArgumentExpressionList<Self>;
    type UnaryExpression = Box<c11::UnaryExpression<Self>>;
    type CastExpression = Box<c11::CastExpression<Self>>;
    type AssignmentExpression = Box<c11::AssignmentExpression<Self>>;
    type Expression = Box<c11::Expression<Self>>;
    type InitDeclaratorListVarname = c11::InitDeclaratorListVarname<Self>;
    type InitDeclaratorListTypedef = c11::InitDeclaratorListTypedef<Self>;
    type StructDeclaratorList = c11::StructDeclaratorList<Self>;
    type Pointer = Box<c11::Pointer<Self>>;
    type TypeQualifierList = c11::TypeQualifierList<Self>;
    type IdentifierList = c11::IdentifierList<Self>;
    type DirectAbstractDeclarator = Box<c11::DirectAbstractDeclarator<Self>>;
    type InitializerList = Box<c11::InitializerList<Self>>;
    type Statement = Box<c11::Statement<Self>>;
    type TranslationUnitFile = Box<c11::TranslationUnitFile<Self>>;
}

use gazelle::Action;

impl Action<c11::TypedefName<Self>> for CActions {
    fn build(&mut self, node: c11::TypedefName<Self>) -> Result<String, Self::Error> {
        let c11::TypedefName::TypedefName(name) = node;
        Ok(name)
    }
}

impl Action<c11::VarName<Self>> for CActions {
    fn build(&mut self, node: c11::VarName<Self>) -> Result<String, Self::Error> {
        let c11::VarName::VarName(name) = node;
        Ok(name)
    }
}

impl Action<c11::GeneralIdentifier<Self>> for CActions {
    fn build(&mut self, node: c11::GeneralIdentifier<Self>) -> Result<String, Self::Error> {
        Ok(match node {
            c11::GeneralIdentifier::Typedef(name) => name,
            c11::GeneralIdentifier::Var(name) => name,
        })
    }
}

impl Action<c11::EnumerationConstant<Self>> for CActions {
    fn build(&mut self, node: c11::EnumerationConstant<Self>) -> Result<String, Self::Error> {
        let c11::EnumerationConstant::EnumConst(name) = node;
        Ok(name)
    }
}

impl Action<c11::SaveContext<Self>> for CActions {
    fn build(&mut self, _: c11::SaveContext<Self>) -> Result<Context, Self::Error> {
        Ok(self.ctx.save())
    }
}

impl Action<c11::ScopedCompoundStatement<Self>> for CActions {
    fn build(
        &mut self,
        mut node: c11::ScopedCompoundStatement<Self>,
    ) -> Result<Node<c11::ScopedCompoundStatement<Self>>, Self::Error> {
        let c11::ScopedCompoundStatement::RestoreCompound(ref mut ctx, _) = node;
        self.ctx.restore(std::mem::take(ctx));
        Ok(Node(node))
    }
}

impl Action<c11::ScopedIterationStatement<Self>> for CActions {
    fn build(
        &mut self,
        mut node: c11::ScopedIterationStatement<Self>,
    ) -> Result<Node<c11::ScopedIterationStatement<Self>>, Self::Error> {
        let c11::ScopedIterationStatement::RestoreIteration(ref mut ctx, _) = node;
        self.ctx.restore(std::mem::take(ctx));
        Ok(Node(node))
    }
}

impl Action<c11::ScopedSelectionStatement<Self>> for CActions {
    fn build(
        &mut self,
        mut node: c11::ScopedSelectionStatement<Self>,
    ) -> Result<Node<c11::ScopedSelectionStatement<Self>>, Self::Error> {
        let c11::ScopedSelectionStatement::RestoreSelection(ref mut ctx, _) = node;
        self.ctx.restore(std::mem::take(ctx));
        Ok(Node(node))
    }
}

impl Action<c11::ScopedStatement<Self>> for CActions {
    fn build(
        &mut self,
        mut node: c11::ScopedStatement<Self>,
    ) -> Result<Node<c11::ScopedStatement<Self>>, Self::Error> {
        let c11::ScopedStatement::RestoreStatement(ref mut ctx, _) = node;
        self.ctx.restore(std::mem::take(ctx));
        Ok(Node(node))
    }
}

impl Action<c11::ScopedParameterTypeList<Self>> for CActions {
    fn build(
        &mut self,
        mut node: c11::ScopedParameterTypeList<Self>,
    ) -> Result<Node<c11::ScopedParameterTypeList<Self>>, Self::Error> {
        let c11::ScopedParameterTypeList::ScopedParams(ref mut ctx, _) = node;
        self.ctx.restore(std::mem::take(ctx));
        Ok(Node(node))
    }
}

impl Action<c11::DeclaratorVarname<Self>> for CActions {
    fn build(
        &mut self,
        node: c11::DeclaratorVarname<Self>,
    ) -> Result<Node<c11::DeclaratorVarname<Self>>, Self::Error> {
        let c11::DeclaratorVarname::DeclVarname(ref d) = node;
        self.ctx.declare_varname(declarator_name(d));
        Ok(Node(node))
    }
}

impl Action<c11::DeclaratorTypedefname<Self>> for CActions {
    fn build(
        &mut self,
        node: c11::DeclaratorTypedefname<Self>,
    ) -> Result<Node<c11::DeclaratorTypedefname<Self>>, Self::Error> {
        let c11::DeclaratorTypedefname::RegisterTypedef(ref d) = node;
        self.ctx.declare_typedef(declarator_name(d));
        Ok(Node(node))
    }
}

impl Action<c11::FunctionDefinition1<Self>> for CActions {
    fn build(
        &mut self,
        mut node: c11::FunctionDefinition1<Self>,
    ) -> Result<(Context, c11::FunctionDefinition1<Self>), Self::Error> {
        let c11::FunctionDefinition1::FuncDef1(_, ref mut dv) = node;
        let c11::DeclaratorVarname::DeclVarname(ref mut d) = dv.0;
        let name = declarator_name(d).to_string();
        let saved = self.ctx.save();
        if let Some(param_ctx) = declarator_param_ctx_take(d) {
            self.ctx.restore(param_ctx);
            self.ctx.declare_varname(&name);
        }
        Ok((saved, node))
    }
}

impl Action<c11::FunctionDefinition<Self>> for CActions {
    fn build(
        &mut self,
        mut node: c11::FunctionDefinition<Self>,
    ) -> Result<Node<c11::FunctionDefinition<Self>>, Self::Error> {
        let c11::FunctionDefinition::FuncDef((ref mut saved, _), _, _) = node;
        self.ctx.restore(std::mem::take(saved));
        Ok(Node(node))
    }
}

impl Action<c11::Enumerator<Self>> for CActions {
    fn build(
        &mut self,
        node: c11::Enumerator<Self>,
    ) -> Result<Node<c11::Enumerator<Self>>, Self::Error> {
        match &node {
            c11::Enumerator::DeclEnum(name) | c11::Enumerator::DeclEnumExpr(name, _) => {
                self.ctx.declare_varname(name);
            }
        }
        Ok(Node(node))
    }
}

// =============================================================================
// C Lexer with Typedef Feedback
// =============================================================================

/// C11 lexer with lexer feedback for typedef disambiguation
type Span = std::ops::Range<usize>;

pub struct C11Lexer<'a> {
    input: &'a str,
    src: gazelle::lexer::Scanner<std::str::Chars<'a>>,
    /// Pending identifier - when Some, next call returns TYPE or VARIABLE
    /// based on is_typedef check at that moment (delayed decision)
    pending_ident: Option<String>,
}

impl<'a> C11Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            src: gazelle::lexer::Scanner::new(input),
            pending_ident: None,
        }
    }

    fn next(
        &mut self,
        ctx: &TypedefContext,
    ) -> Result<Option<(c11::Terminal<CActions>, Span)>, String> {
        // If we have a pending identifier, emit TYPE or VARIABLE based on current context
        if let Some(id) = self.pending_ident.take() {
            let off = self.src.offset();
            let term = if ctx.is_typedef(&id) {
                c11::Terminal::Type
            } else {
                c11::Terminal::Variable
            };
            return Ok(Some((term, off..off)));
        }

        // Skip whitespace and comments
        self.src.skip_whitespace();
        while self.src.skip_line_comment("//") || self.src.skip_block_comment("/*", "*/") {
            self.src.skip_whitespace();
        }

        let start = self.src.offset();
        macro_rules! ok {
            ($t:expr) => {
                Ok(Some(($t, start..self.src.offset())))
            };
        }

        if self.src.at_end() {
            return Ok(None);
        }

        // Identifier or keyword
        if let Some(span) = self.src.read_ident() {
            let s = &self.input[span.clone()];

            // Check for C-style prefixed string/char literals: L, u, U, u8
            if matches!(s, "L" | "u" | "U" | "u8") {
                if self.src.peek() == Some('\'') {
                    self.src.read_string_raw('\'').map_err(|e| e.to_string())?;
                    return ok!(c11::Terminal::Constant(String::new()));
                } else if self.src.peek() == Some('"') {
                    self.src.read_string_raw('"').map_err(|e| e.to_string())?;
                    return ok!(c11::Terminal::StringLiteral(String::new()));
                }
            }

            let term = match s {
                // Keywords
                "auto" => c11::Terminal::Auto,
                "break" => c11::Terminal::Break,
                "case" => c11::Terminal::Case,
                "char" => c11::Terminal::Char,
                "const" => c11::Terminal::Const,
                "continue" => c11::Terminal::Continue,
                "default" => c11::Terminal::Default,
                "do" => c11::Terminal::Do,
                "double" => c11::Terminal::Double,
                "else" => c11::Terminal::Else,
                "enum" => c11::Terminal::Enum,
                "extern" => c11::Terminal::Extern,
                "float" => c11::Terminal::Float,
                "for" => c11::Terminal::For,
                "goto" => c11::Terminal::Goto,
                "if" => c11::Terminal::If,
                "inline" => c11::Terminal::Inline,
                "int" => c11::Terminal::Int,
                "long" => c11::Terminal::Long,
                "register" => c11::Terminal::Register,
                "restrict" => c11::Terminal::Restrict,
                "return" => c11::Terminal::Return,
                "short" => c11::Terminal::Short,
                "signed" => c11::Terminal::Signed,
                "sizeof" => c11::Terminal::Sizeof,
                "static" => c11::Terminal::Static,
                "struct" => c11::Terminal::Struct,
                "switch" => c11::Terminal::Switch,
                "typedef" => c11::Terminal::Typedef,
                "union" => c11::Terminal::Union,
                "unsigned" => c11::Terminal::Unsigned,
                "void" => c11::Terminal::Void,
                "volatile" => c11::Terminal::Volatile,
                "while" => c11::Terminal::While,
                // C11 keywords
                "_Alignas" => c11::Terminal::Alignas,
                "_Alignof" => c11::Terminal::Alignof,
                "_Atomic" => c11::Terminal::Atomic,
                "_Bool" => c11::Terminal::Bool,
                "_Complex" => c11::Terminal::Complex,
                "_Generic" => c11::Terminal::Generic,
                "_Imaginary" => c11::Terminal::Imaginary,
                "_Noreturn" => c11::Terminal::Noreturn,
                "_Static_assert" => c11::Terminal::StaticAssert,
                "_Thread_local" => c11::Terminal::ThreadLocal,
                // Identifier - queue TYPE/VARIABLE for next call
                _ => {
                    self.pending_ident = Some(s.to_string());
                    c11::Terminal::Name(s.to_string())
                }
            };
            return ok!(term);
        }

        // Number or character literal -> CONSTANT
        if self.src.read_digits().is_some() {
            return ok!(c11::Terminal::Constant(String::new()));
        }

        // String literal
        if self.src.peek() == Some('"') {
            self.src.read_string_raw('"').map_err(|e| e.to_string())?;
            return ok!(c11::Terminal::StringLiteral(String::new()));
        }

        // Character literal
        if self.src.peek() == Some('\'') {
            self.src.read_string_raw('\'').map_err(|e| e.to_string())?;
            return ok!(c11::Terminal::Constant(String::new()));
        }

        // Punctuation
        if let Some(c) = self.src.peek() {
            match c {
                '(' => {
                    self.src.advance();
                    return ok!(c11::Terminal::Lparen);
                }
                ')' => {
                    self.src.advance();
                    return ok!(c11::Terminal::Rparen);
                }
                '{' => {
                    self.src.advance();
                    return ok!(c11::Terminal::Lbrace);
                }
                '}' => {
                    self.src.advance();
                    return ok!(c11::Terminal::Rbrace);
                }
                '[' => {
                    self.src.advance();
                    return ok!(c11::Terminal::Lbrack);
                }
                ']' => {
                    self.src.advance();
                    return ok!(c11::Terminal::Rbrack);
                }
                ';' => {
                    self.src.advance();
                    return ok!(c11::Terminal::Semicolon);
                }
                ',' => {
                    self.src.advance();
                    return ok!(c11::Terminal::Comma);
                }
                _ => {}
            }
        }

        // Multi-char operators (longest first for maximal munch)
        const MULTI_OPS: &[&str] = &[
            "...", "<<=", ">>=", // 0-2: three-char
            "->", "++", "--", // 3-5: special two-char
            "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", // 6-13: assign
            "||", "&&", "==", "!=", "<=", ">=", "<<", ">>", // 14-21: binary
        ];
        const MULTI_PREC: &[Option<Precedence>] = &[
            None,
            Some(Precedence::Right(1)),
            Some(Precedence::Right(1)), // 0-2
            None,
            None,
            None, // 3-5: PTR, INC, DEC (no prec)
            Some(Precedence::Right(1)),
            Some(Precedence::Right(1)),
            Some(Precedence::Right(1)),
            Some(Precedence::Right(1)),
            Some(Precedence::Right(1)),
            Some(Precedence::Right(1)),
            Some(Precedence::Right(1)),
            Some(Precedence::Right(1)), // 6-13
            Some(Precedence::Left(3)),
            Some(Precedence::Left(4)),
            Some(Precedence::Left(8)),
            Some(Precedence::Left(8)),
            Some(Precedence::Left(9)),
            Some(Precedence::Left(9)),
            Some(Precedence::Left(10)),
            Some(Precedence::Left(10)), // 14-21
        ];

        if let Some((idx, _span)) = self.src.read_one_of(MULTI_OPS) {
            return ok!(match idx {
                0 => c11::Terminal::Ellipsis,
                3 => c11::Terminal::Ptr,
                4 => c11::Terminal::Inc,
                5 => c11::Terminal::Dec,
                _ => {
                    let p = MULTI_PREC[idx].unwrap();
                    c11::Terminal::Binop(p, p)
                }
            });
        }

        // Single-char operators
        if let Some(c) = self.src.peek() {
            self.src.advance();
            return ok!(match c {
                ':' => c11::Terminal::Colon,
                '.' => c11::Terminal::Dot,
                '~' => c11::Terminal::Tilde,
                '!' => c11::Terminal::Bang,
                '=' => c11::Terminal::Eq(Precedence::Right(1)),
                '?' => c11::Terminal::Question(Precedence::Right(2)),
                '|' => {
                    let p = Precedence::Left(5);
                    c11::Terminal::Binop(p, p)
                }
                '^' => {
                    let p = Precedence::Left(6);
                    c11::Terminal::Binop(p, p)
                }
                '&' => c11::Terminal::Amp(Precedence::Left(7)),
                '<' => {
                    let p = Precedence::Left(9);
                    c11::Terminal::Binop(p, p)
                }
                '>' => {
                    let p = Precedence::Left(9);
                    c11::Terminal::Binop(p, p)
                }
                '+' => c11::Terminal::Plus(Precedence::Left(11)),
                '-' => c11::Terminal::Minus(Precedence::Left(11)),
                '*' => c11::Terminal::Star(Precedence::Left(12)),
                '/' => {
                    let p = Precedence::Left(12);
                    c11::Terminal::Binop(p, p)
                }
                '%' => {
                    let p = Precedence::Left(12);
                    c11::Terminal::Binop(p, p)
                }
                _ => return Err(format!("Unknown character: {}", c)),
            });
        }

        Ok(None)
    }
}

// =============================================================================
// Parse Function
// =============================================================================

type Cst = Box<c11::TranslationUnitFile<CActions>>;

fn c11_display_names() -> Vec<(&'static str, &'static str)> {
    vec![
        // Identifiers/literals
        ("NAME", "identifier"),
        ("TYPE", "type-name marker"),
        ("VARIABLE", "variable marker"),
        ("CONSTANT", "constant"),
        ("STRING_LITERAL", "string literal"),
        // Keywords
        ("IF", "'if'"),
        ("ELSE", "'else'"),
        ("RETURN", "'return'"),
        ("INT", "'int'"),
        ("VOID", "'void'"),
        ("STRUCT", "'struct'"),
        ("UNION", "'union'"),
        ("TYPEDEF", "'typedef'"),
        ("WHILE", "'while'"),
        ("FOR", "'for'"),
        ("DO", "'do'"),
        ("SWITCH", "'switch'"),
        ("CASE", "'case'"),
        ("DEFAULT", "'default'"),
        // Punctuation/operators
        ("LPAREN", "'('"),
        ("RPAREN", "')'"),
        ("LBRACE", "'{'"),
        ("RBRACE", "'}'"),
        ("LBRACK", "'['"),
        ("RBRACK", "']'"),
        ("SEMICOLON", "';'"),
        ("COLON", "':'"),
        ("COMMA", "','"),
        ("DOT", "'.'"),
        ("PTR", "'->'"),
        ("ELLIPSIS", "'...'"),
        ("INC", "'++'"),
        ("DEC", "'--'"),
        ("TILDE", "'~'"),
        ("BANG", "'!'"),
        ("EQ", "assignment operator"),
        ("QUESTION", "'?'"),
        ("STAR", "'*'"),
        ("AMP", "'&'"),
        ("PLUS", "'+'"),
        ("MINUS", "'-'"),
        ("BINOP", "binary operator"),
        // Common non-terminals shown in expected sets
        ("assignment_expression", "expression"),
        ("binary_op", "binary operator"),
        ("block_item", "declaration or statement"),
        ("translation_unit_file", "translation unit"),
        // Common synthetic helper
        ("__block_item_star", "declaration-or-statement*"),
    ]
}

/// Parse C11 source code
pub fn parse(input: &str) -> Result<Cst, String> {
    // Strip preprocessor lines (lines starting with #)
    let preprocessed = input
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    parse_impl(&preprocessed)
}

/// Parse C11 source code
fn parse_impl(input: &str) -> Result<Cst, String> {
    let mut parser = c11::Parser::<CActions>::new();
    let mut actions = CActions::new();
    let mut lexer = C11Lexer::new(input);
    let display_names = c11_display_names();
    let mut token_texts: Vec<String> = Vec::new();

    loop {
        let tok = lexer.next(&actions.ctx)?;
        match tok {
            Some((t, span)) => {
                token_texts.push(input[span.clone()].to_string());
                parser.push(t, &mut actions).map_err(|e| {
                    let (line, col) = lexer.src.line_col(span.start);
                    let tokens: Vec<&str> = token_texts.iter().map(String::as_str).collect();
                    format!("Parse error at {line}:{col}: {}", {
                        let gazelle::ParseError::Syntax { terminal } = e;
                        parser.format_error(terminal, Some(&display_names), Some(&tokens))
                    },)
                })?;
            }
            None => break,
        }
    }

    let tokens: Vec<&str> = token_texts.iter().map(String::as_str).collect();
    parser
        .finish(&mut actions)
        .map_err(|(p, gazelle::ParseError::Syntax { terminal })| {
            format!(
                "Finish error: {}",
                p.format_error(terminal, Some(&display_names), Some(&tokens))
            )
        })
}

/// A located, displayable error from recovery.
#[derive(Debug)]
struct RecoveryError {
    line: usize,
    col: usize,
    line_text: String,
    token_text: Option<String>,
    marker_width: usize,
    repairs: Vec<gazelle::Repair>,
}

struct RecoveryResult {
    errors: Vec<RecoveryError>,
}

/// Parse C11 source code with error recovery — reports all errors found.
fn parse_with_recovery(input: &str) -> RecoveryResult {
    // Strip preprocessor lines
    let preprocessed: String = input
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");

    let mut parser = c11::Parser::<CActions>::new();
    let mut actions = CActions::new();
    let mut lexer = C11Lexer::new(&preprocessed);
    let mut spans: Vec<Span> = Vec::new();

    loop {
        let (tok, span) = match lexer.next(&actions.ctx) {
            Ok(Some(t)) => t,
            Ok(None) => break,
            Err(e) => {
                eprintln!("Lex error: {}", e);
                break;
            }
        };

        spans.push(span);

        let raw_token = gazelle::Token {
            terminal: tok.symbol_id(),
            resolution: tok.resolution(),
        };
        match parser.push(tok, &mut actions) {
            Ok(()) => {}
            Err(_) => {
                let error_idx = spans.len() - 1;
                let mut buffer = vec![raw_token];

                while let Ok(Some((t, span))) = lexer.next(&actions.ctx) {
                    spans.push(span);
                    buffer.push(gazelle::Token {
                        terminal: t.symbol_id(),
                        resolution: t.resolution(),
                    });
                }

                let errors = parser.recover(&buffer);
                return to_result(&preprocessed, &lexer, &spans, errors, error_idx);
            }
        }
    }

    // Try to finish
    match parser.finish(&mut actions) {
        Ok(_) => RecoveryResult { errors: vec![] },
        Err((mut p, _)) => {
            let errors = p.recover(&[]);
            to_result(&preprocessed, &lexer, &spans, errors, spans.len())
        }
    }
}

/// Convert raw RecoveryInfo into displayable errors using the lexer's line/col tracking.
fn to_result(
    source: &str,
    lexer: &C11Lexer,
    spans: &[Span],
    errors: Vec<gazelle::RecoveryInfo>,
    base: usize,
) -> RecoveryResult {
    let lines: Vec<&str> = source.lines().collect();
    let errors = errors
        .into_iter()
        .map(|e| {
            let pos = e.position + base;
            if pos < spans.len() {
                let (line, col) = lexer.src.line_col(spans[pos].start);
                let marker_width = if e
                    .repairs
                    .iter()
                    .any(|repair| matches!(repair, gazelle::Repair::Delete(_)))
                {
                    source[spans[pos].clone()].chars().count().max(1)
                } else {
                    1
                };
                RecoveryError {
                    line,
                    col,
                    line_text: lines.get(line - 1).unwrap_or(&"").to_string(),
                    token_text: Some(source[spans[pos].clone()].to_string()),
                    marker_width,
                    repairs: e.repairs,
                }
            } else {
                let (line, col) = lexer.src.line_col(source.len());
                RecoveryError {
                    line,
                    col,
                    line_text: lines.get(line.saturating_sub(1)).unwrap_or(&"").to_string(),
                    token_text: None,
                    marker_width: 1,
                    repairs: e.repairs,
                }
            }
        })
        .collect();
    RecoveryResult { errors }
}

// =============================================================================
// Main
// =============================================================================

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        println!("C11 Parser with Error Recovery");
        println!("Usage: c11 <file.c>");
        println!();
        println!("Run tests with: cargo test --example c11");
        return;
    }

    let path = &args[1];
    let input = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read {}: {}", path, e);
            std::process::exit(1);
        }
    };

    // First try normal parse
    match parse(&input) {
        Ok(_) => {
            println!("{}: parsed successfully", path);
        }
        Err(first_err) => {
            println!("{}: {}", path, first_err);
            println!();
            println!("Attempting error recovery...");
            use gazelle::ErrorContext;
            let ctx = c11::Parser::<CActions>::error_info();
            let display_names = c11_display_names();
            let result = parse_with_recovery(&input);
            println!("Found {} error(s):", result.errors.len());
            for err in &result.errors {
                let repair_strs: Vec<String> = err
                    .repairs
                    .iter()
                    .map(|r| match r {
                        gazelle::Repair::Insert(id) => {
                            let name = ctx.symbol_name(*id);
                            let shown = display_names
                                .iter()
                                .find(|(k, _)| *k == name)
                                .map(|(_, v)| *v)
                                .unwrap_or(name);
                            match &err.token_text {
                                Some(token) => format!("insert {} before '{}'", shown, token),
                                None => format!("insert {}", shown),
                            }
                        }
                        gazelle::Repair::Delete(id) => {
                            let name = ctx.symbol_name(*id);
                            let shown = display_names
                                .iter()
                                .find(|(k, _)| *k == name)
                                .map(|(_, v)| *v)
                                .unwrap_or(name);
                            match &err.token_text {
                                Some(token) => format!("delete '{}'", token),
                                None => format!("delete {}", shown),
                            }
                        }
                        gazelle::Repair::Shift => "shift".to_string(),
                    })
                    .collect();
                if err.line > 0 {
                    println!(
                        "  {}:{}:{}: {}",
                        path,
                        err.line,
                        err.col,
                        repair_strs.join(", ")
                    );
                    println!("    {}", err.line_text);
                    println!(
                        "    {}{}",
                        " ".repeat(err.col - 1),
                        "^".repeat(err.marker_width)
                    );
                } else {
                    println!("  {}:EOF: {}", path, repair_strs.join(", "));
                }
            }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod recovery_benchmark;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_expression() {
        let mut lexer = C11Lexer::new("int x;");
        let ctx = TypedefContext::new();

        assert!(matches!(
            lexer.next(&ctx).unwrap(),
            Some((c11::Terminal::Int, _))
        ));
        assert!(matches!(
            lexer.next(&ctx).unwrap(),
            Some((c11::Terminal::Name(_), _))
        ));
        assert!(matches!(
            lexer.next(&ctx).unwrap(),
            Some((c11::Terminal::Variable, _))
        ));
        assert!(matches!(
            lexer.next(&ctx).unwrap(),
            Some((c11::Terminal::Semicolon, _))
        ));
    }

    #[test]
    fn test_typedef_context() {
        let mut ctx = TypedefContext::new();
        assert!(!ctx.is_typedef("T"));

        ctx.declare_typedef("T");
        assert!(ctx.is_typedef("T"));

        // Save context, add S, then restore
        let saved = ctx.save();
        assert!(ctx.is_typedef("T"));
        ctx.declare_typedef("S");
        assert!(ctx.is_typedef("S"));

        ctx.restore(saved);
        assert!(!ctx.is_typedef("S"));
        assert!(ctx.is_typedef("T"));
    }

    #[test]
    fn test_typedef_shadowing() {
        let mut ctx = TypedefContext::new();
        ctx.declare_typedef("T");
        assert!(ctx.is_typedef("T"));

        // Shadow T with a variable declaration
        ctx.declare_varname("T");
        assert!(!ctx.is_typedef("T"));
    }

    #[test]
    fn test_local_scope_simple() {
        // Simplified version of local_scope.c
        let code = r#"
typedef int T;
void f(void) {
  T y = 1;
}
"#;
        parse(code).expect("basic function with typedef should parse");
    }

    #[test]
    fn test_local_scope_with_shadow() {
        // Local variable shadows typedef
        let code = r#"
typedef int T;
void f(void) {
  int T;
  T = 1;
}
"#;
        parse(code).expect("typedef shadow should parse");
    }

    #[test]
    fn test_local_scope_with_if() {
        // Scoped shadow in if block
        let code = r#"
typedef int T;
void f(void) {
  if(1) {
    int T;
    T = 1;
  }
  T x = 1;
}
"#;
        let preprocessed = code
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        parse_impl(&preprocessed).expect("scoped typedef shadow should parse");
    }

    // Note: argument_scope test requires tracking context through declarators
    // (like Jourdan's reinstall_function_context). This is a known limitation.

    #[test]
    fn test_typedef_lexer_feedback() {
        let mut ctx = TypedefContext::new();
        ctx.declare_typedef("MyType");

        let mut lexer = C11Lexer::new("MyType x");

        let tok1 = lexer.next(&ctx).unwrap();
        assert!(matches!(tok1, Some((c11::Terminal::Name(_), _))));

        let tok2 = lexer.next(&ctx).unwrap();
        assert!(matches!(tok2, Some((c11::Terminal::Type, _))));

        let tok3 = lexer.next(&ctx).unwrap();
        assert!(matches!(tok3, Some((c11::Terminal::Name(_), _))));

        let tok4 = lexer.next(&ctx).unwrap();
        assert!(matches!(tok4, Some((c11::Terminal::Variable, _))));
    }

    #[test]
    fn test_keywords() {
        let ctx = TypedefContext::new();
        let mut lexer = C11Lexer::new("int void struct typedef if while for");

        assert!(matches!(
            lexer.next(&ctx).unwrap(),
            Some((c11::Terminal::Int, _))
        ));
        assert!(matches!(
            lexer.next(&ctx).unwrap(),
            Some((c11::Terminal::Void, _))
        ));
        assert!(matches!(
            lexer.next(&ctx).unwrap(),
            Some((c11::Terminal::Struct, _))
        ));
        assert!(matches!(
            lexer.next(&ctx).unwrap(),
            Some((c11::Terminal::Typedef, _))
        ));
        assert!(matches!(
            lexer.next(&ctx).unwrap(),
            Some((c11::Terminal::If, _))
        ));
        assert!(matches!(
            lexer.next(&ctx).unwrap(),
            Some((c11::Terminal::While, _))
        ));
        assert!(matches!(
            lexer.next(&ctx).unwrap(),
            Some((c11::Terminal::For, _))
        ));
    }

    // =========================================================================
    // C11parser test suite
    // =========================================================================

    /// Helper to parse a C file and report success/failure
    fn parse_c_file(path: &str) -> Result<Cst, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path, e))?;
        parse(&content).map_err(|e| format!("{}: {}", path, e))
    }

    #[test]
    fn test_recovery_missing_semi() {
        let result = parse_with_recovery("int x\nint y;");
        eprintln!("errors: {:?}", result.errors);
        assert!(!result.errors.is_empty(), "expected at least one error");
    }

    #[test]
    fn test_simple_parse() {
        let cst = parse_impl("int;").unwrap();
        eprintln!("{:#?}", cst);

        let cst = parse_impl("typedef int T;").unwrap();
        eprintln!("{:#?}", cst);
    }

    #[test]
    fn test_cst_showcase() {
        let cases = &[
            ("typedef + variable decl", "typedef int T; T x;"),
            ("struct definition", "struct Point { int x; int y; };"),
            ("enum with values", "enum Color { RED, GREEN = 2, BLUE };"),
            ("pointer declarator", "int **p;"),
            ("function pointer", "void (*fp)(int, char);"),
            ("typedef struct", "typedef struct { int x; } Point;"),
            (
                "function with if",
                "int f(int x) { if (x) return x + 1; return 0; }",
            ),
        ];
        for (name, code) in cases {
            let cst = parse_impl(code).unwrap();
            eprintln!("=== {} ===\n{:#?}\n", name, cst);
        }
    }

    /// Test files that should parse successfully
    const C11_TEST_FILES: &[&str] = &[
        "examples/c11/C11parser/tests/typedef_star.c",
        "examples/c11/C11parser/tests/variable_star.c",
        "examples/c11/C11parser/tests/local_typedef.c",
        "examples/c11/C11parser/tests/block_scope.c",
        "examples/c11/C11parser/tests/declaration_ambiguity.c",
        "examples/c11/C11parser/tests/enum.c",
        "examples/c11/C11parser/tests/enum_shadows_typedef.c",
        "examples/c11/C11parser/tests/enum_constant_visibility.c",
        "examples/c11/C11parser/tests/namespaces.c",
        "examples/c11/C11parser/tests/local_scope.c",
        "examples/c11/C11parser/tests/if_scopes.c",
        "examples/c11/C11parser/tests/loop_scopes.c",
        "examples/c11/C11parser/tests/no_local_scope.c",
        "examples/c11/C11parser/tests/function_parameter_scope.c",
        "examples/c11/C11parser/tests/function_parameter_scope_extends.c",
        "examples/c11/C11parser/tests/argument_scope.c",
        "examples/c11/C11parser/tests/control-scope.c",
        "examples/c11/C11parser/tests/dangling_else.c",
        "examples/c11/C11parser/tests/dangling_else_lookahead.c",
        "examples/c11/C11parser/tests/dangling_else_lookahead.if.c",
        "examples/c11/C11parser/tests/parameter_declaration_ambiguity.c",
        "examples/c11/C11parser/tests/parameter_declaration_ambiguity.test.c",
        "examples/c11/C11parser/tests/bitfield_declaration_ambiguity.c",
        "examples/c11/C11parser/tests/bitfield_declaration_ambiguity.ok.c",
        "examples/c11/C11parser/tests/expressions.c",
        "examples/c11/C11parser/tests/statements.c",
        "examples/c11/C11parser/tests/types.c",
        "examples/c11/C11parser/tests/declarators.c",
        "examples/c11/C11parser/tests/designator.c",
        "examples/c11/C11parser/tests/function-decls.c",
        "examples/c11/C11parser/tests/struct-recursion.c",
        "examples/c11/C11parser/tests/long-long-struct.c",
        "examples/c11/C11parser/tests/c-namespace.c",
        "examples/c11/C11parser/tests/enum-trick.c",
        "examples/c11/C11parser/tests/char-literal-printing.c",
        // C11-specific features
        "examples/c11/C11parser/tests/c11-noreturn.c",
        "examples/c11/C11parser/tests/c1x-alignas.c",
        "examples/c11/C11parser/tests/atomic.c",
        "examples/c11/C11parser/tests/atomic_parenthesis.c",
        "examples/c11/C11parser/tests/aligned_struct_c18.c",
        "examples/c11/C11parser/tests/declarator_visibility.c",
    ];

    #[test]
    fn test_c11parser_suite() {
        let mut passed = 0;
        let mut failed = Vec::new();

        for file in C11_TEST_FILES {
            match parse_c_file(file) {
                Ok(_) => {
                    passed += 1;
                    println!("PASS: {}", file);
                }
                Err(e) => {
                    failed.push(e);
                    println!("FAIL: {}", file);
                }
            }
        }

        println!("\n{} passed, {} failed", passed, failed.len());

        if !failed.is_empty() {
            for err in &failed {
                eprintln!("FAIL: {}", err);
            }
            panic!("{} tests failed", failed.len());
        }
    }

    // =========================================================================
    // Expression evaluation tests - verify precedence using actual C11 lexer
    // =========================================================================

    /// Operators that go through the BINOP terminal (those without their own terminal).
    #[derive(Clone, Copy, Debug)]
    #[allow(dead_code)]
    enum BinOp {
        Or,
        And,
        BitOr,
        BitXor,
        Assign,
        Eq,
        Ne,
        Lt,
        Gt,
        Le,
        Ge,
        Shl,
        Shr,
        Div,
        Mod,
    }

    // Minimal expression grammar for evaluation testing
    // Flat expression grammar — runtime precedence handles everything.
    // No term/expr split needed. The lexer sets precedence based on one bit
    // of context: "was the last token expression-ending?"

    gazelle! {
        #[derive(Debug)]
        grammar expr {
            start expr;
            terminals {
                NUM: _,
                LPAREN, RPAREN,
                COLON,
                BANG, TILDE,
                prec PLUS,
                prec MINUS,
                prec STAR,
                prec AMP,
                prec PREFIX: _,
                prec POSTFIX: _,  // INC/DEC
                prec QUESTION,
                prec BINOP: _
            }

            prefix_op = PREFIX => prefix | PLUS => pos | MINUS => neg | STAR => deref | AMP => addr | BANG => lognot | TILDE => bitnot;
            binary_op = BINOP => binop | PLUS => add | MINUS => sub | STAR => mul | AMP => bitand;

            expr = NUM => num
                 | LPAREN expr RPAREN => paren
                 | prefix_op expr => prefix
                 | expr POSTFIX => postfix
                 | expr binary_op expr => binop
                 | expr QUESTION expr COLON expr => ternary;
        }
    }

    struct Eval;

    #[derive(Clone, Copy, Debug)]
    enum IncDec {
        Inc,
        Dec,
    }

    impl gazelle::ErrorType for Eval {
        type Error = core::convert::Infallible;
    }

    impl expr::Types for Eval {
        type Num = i64;
        type Prefix = IncDec;
        type Postfix = IncDec;
        type Binop = BinOp;
        type PrefixOp = expr::PrefixOp<Self>;
        type BinaryOp = expr::BinaryOp<Self>;
        type Expr = i64;
    }

    impl gazelle::Action<expr::Expr<Self>> for Eval {
        fn build(&mut self, node: expr::Expr<Self>) -> Result<i64, Self::Error> {
            Ok(match node {
                expr::Expr::Num(n) => n,
                expr::Expr::Paren(e) => e,
                expr::Expr::Prefix(expr::PrefixOp::Neg, e) => -e,
                expr::Expr::Prefix(expr::PrefixOp::Bitnot, e) => !e,
                expr::Expr::Prefix(expr::PrefixOp::Lognot, e) => {
                    if e == 0 {
                        1
                    } else {
                        0
                    }
                }
                expr::Expr::Prefix(expr::PrefixOp::Prefix(IncDec::Inc), e) => e + 1,
                expr::Expr::Prefix(expr::PrefixOp::Prefix(IncDec::Dec), e) => e - 1,
                expr::Expr::Prefix(_, e) => e,
                expr::Expr::Postfix(e, _) => e,
                expr::Expr::Binop(l, expr::BinaryOp::Add, r) => l + r,
                expr::Expr::Binop(l, expr::BinaryOp::Sub, r) => l - r,
                expr::Expr::Binop(l, expr::BinaryOp::Mul, r) => l * r,
                expr::Expr::Binop(l, expr::BinaryOp::Bitand, r) => l & r,
                expr::Expr::Binop(l, expr::BinaryOp::Binop(BinOp::Div), r) => l / r,
                expr::Expr::Binop(l, expr::BinaryOp::Binop(BinOp::Mod), r) => l % r,
                expr::Expr::Binop(l, expr::BinaryOp::Binop(BinOp::Shl), r) => l << r,
                expr::Expr::Binop(l, expr::BinaryOp::Binop(BinOp::Shr), r) => l >> r,
                expr::Expr::Binop(l, expr::BinaryOp::Binop(BinOp::BitOr), r) => l | r,
                expr::Expr::Binop(l, expr::BinaryOp::Binop(BinOp::BitXor), r) => l ^ r,
                expr::Expr::Binop(l, expr::BinaryOp::Binop(BinOp::Lt), r) => {
                    if l < r {
                        1
                    } else {
                        0
                    }
                }
                expr::Expr::Binop(l, expr::BinaryOp::Binop(BinOp::Gt), r) => {
                    if l > r {
                        1
                    } else {
                        0
                    }
                }
                expr::Expr::Binop(l, expr::BinaryOp::Binop(BinOp::Le), r) => {
                    if l <= r {
                        1
                    } else {
                        0
                    }
                }
                expr::Expr::Binop(l, expr::BinaryOp::Binop(BinOp::Ge), r) => {
                    if l >= r {
                        1
                    } else {
                        0
                    }
                }
                expr::Expr::Binop(l, expr::BinaryOp::Binop(BinOp::Eq), r) => {
                    if l == r {
                        1
                    } else {
                        0
                    }
                }
                expr::Expr::Binop(l, expr::BinaryOp::Binop(BinOp::Ne), r) => {
                    if l != r {
                        1
                    } else {
                        0
                    }
                }
                expr::Expr::Binop(l, expr::BinaryOp::Binop(BinOp::Or), r) => {
                    if l != 0 || r != 0 {
                        1
                    } else {
                        0
                    }
                }
                expr::Expr::Binop(l, expr::BinaryOp::Binop(BinOp::And), r) => {
                    if l != 0 && r != 0 {
                        1
                    } else {
                        0
                    }
                }
                expr::Expr::Binop(_, expr::BinaryOp::Binop(BinOp::Assign), r) => r,
                expr::Expr::Binop(_, _, r) => r,
                expr::Expr::Ternary(c, t, e) => {
                    if c != 0 {
                        t
                    } else {
                        e
                    }
                }
            })
        }
    }
    struct Show;

    impl gazelle::ErrorType for Show {
        type Error = core::convert::Infallible;
    }

    impl expr::Types for Show {
        type Num = i64;
        type Prefix = IncDec;
        type Postfix = IncDec;
        type Binop = BinOp;
        type PrefixOp = expr::PrefixOp<Self>;
        type BinaryOp = expr::BinaryOp<Self>;
        type Expr = Box<expr::Expr<Self>>;
    }

    fn show_c_expr(input: &str) -> Box<expr::Expr<Show>> {
        let tokens = tokenize_c_expr(input);
        let mut parser = expr::Parser::<Show>::new();
        let mut actions = Show;
        for tok in tokens {
            // Safety: Eval and Show have identical terminal payload types
            let tok: expr::Terminal<Show> = unsafe { std::mem::transmute(tok) };
            parser.push(tok, &mut actions).unwrap();
        }
        parser.finish(&mut actions).ok().unwrap()
    }

    fn tokenize_c_expr(input: &str) -> Vec<expr::Terminal<Eval>> {
        use gazelle::lexer::Scanner;

        let mut src = Scanner::new(input);
        let mut tokens = Vec::new();

        loop {
            src.skip_whitespace();
            if src.at_end() {
                break;
            }

            // Number
            if let Some(span) = src.read_digits() {
                let s = &input[span];
                let n: i64 = s.parse().unwrap_or(0);
                tokens.push(expr::Terminal::Num(n));
                continue;
            }

            // Punctuation
            if let Some(c) = src.peek() {
                match c {
                    '(' => {
                        src.advance();
                        tokens.push(expr::Terminal::Lparen);
                        continue;
                    }
                    ')' => {
                        src.advance();
                        tokens.push(expr::Terminal::Rparen);
                        continue;
                    }
                    _ => {}
                }
            }

            // Multi-char operators
            if src.read_exact("||").is_some() {
                tokens.push(expr::Terminal::Binop(BinOp::Or, Precedence::Left(3)));
                continue;
            }
            if src.read_exact("&&").is_some() {
                tokens.push(expr::Terminal::Binop(BinOp::And, Precedence::Left(4)));
                continue;
            }
            if src.read_exact("==").is_some() {
                tokens.push(expr::Terminal::Binop(BinOp::Eq, Precedence::Left(8)));
                continue;
            }
            if src.read_exact("!=").is_some() {
                tokens.push(expr::Terminal::Binop(BinOp::Ne, Precedence::Left(8)));
                continue;
            }
            if src.read_exact("<=").is_some() {
                tokens.push(expr::Terminal::Binop(BinOp::Le, Precedence::Left(9)));
                continue;
            }
            if src.read_exact(">=").is_some() {
                tokens.push(expr::Terminal::Binop(BinOp::Ge, Precedence::Left(9)));
                continue;
            }
            if src.read_exact("<<").is_some() {
                tokens.push(expr::Terminal::Binop(BinOp::Shl, Precedence::Left(10)));
                continue;
            }
            if src.read_exact(">>").is_some() {
                tokens.push(expr::Terminal::Binop(BinOp::Shr, Precedence::Left(10)));
                continue;
            }
            if src.read_exact("++").is_some() {
                tokens.push(expr::Terminal::Prefix(IncDec::Inc, Precedence::Right(14)));
                continue;
            }
            if src.read_exact("--").is_some() {
                tokens.push(expr::Terminal::Prefix(IncDec::Dec, Precedence::Right(14)));
                continue;
            }

            // Single-char operators
            if let Some(c) = src.peek() {
                src.advance();
                let tok = match c {
                    '?' => expr::Terminal::Question(Precedence::Right(2)),
                    ':' => expr::Terminal::Colon,
                    '|' => expr::Terminal::Binop(BinOp::BitOr, Precedence::Left(5)),
                    '^' => expr::Terminal::Binop(BinOp::BitXor, Precedence::Left(6)),
                    '&' => expr::Terminal::Amp(Precedence::Left(7)),
                    '<' => expr::Terminal::Binop(BinOp::Lt, Precedence::Left(9)),
                    '>' => expr::Terminal::Binop(BinOp::Gt, Precedence::Left(9)),
                    '+' => expr::Terminal::Plus(Precedence::Left(11)),
                    '-' => expr::Terminal::Minus(Precedence::Left(11)),
                    '*' => expr::Terminal::Star(Precedence::Left(12)),
                    '/' => expr::Terminal::Binop(BinOp::Div, Precedence::Left(12)),
                    '%' => expr::Terminal::Binop(BinOp::Mod, Precedence::Left(12)),
                    '=' => expr::Terminal::Binop(BinOp::Assign, Precedence::Right(1)),
                    '~' => expr::Terminal::Tilde,
                    '!' => expr::Terminal::Bang,
                    _ => continue,
                };
                tokens.push(tok);
            }
        }

        tokens
    }

    fn eval_c_expr(input: &str) -> Result<i64, String> {
        let tokens = tokenize_c_expr(input);
        let mut parser = expr::Parser::<Eval>::new();
        let mut actions = Eval;
        for tok in tokens {
            parser
                .push(tok, &mut actions)
                .map_err(|e| format!("{:?}", e))?;
        }
        parser
            .finish(&mut actions)
            .map_err(|(p, gazelle::ParseError::Syntax { terminal })| {
                p.format_error(terminal, None, None)
            })
    }

    // TODO: fix prefix vs binary disambiguation — `-1 * 2` parses as `-(1*2)` instead of `(-1)*2`
    // because MINUS always gets Left(11) (binary precedence). Need parser feedback to assign
    // Right(14) in prefix position.
    #[test]
    #[ignore]
    fn test_expr_parse_tree() {
        // -1 * 2 should be (-1) * 2, not -(1 * 2)
        let tree = format!("{:?}", show_c_expr("-1 * 2"));
        assert!(
            tree.starts_with("Binop(Prefix(Neg,"),
            "expected (-1) * 2, got: {}",
            tree
        );
    }

    #[test]
    fn test_expr_precedence() {
        // Multiplicative > Additive
        assert_eq!(eval_c_expr("1 + 2 * 3").unwrap(), 7);
        assert_eq!(eval_c_expr("2 * 3 + 1").unwrap(), 7);
        assert_eq!(eval_c_expr("(1 + 2) * 3").unwrap(), 9);
    }

    #[test]
    fn test_expr_associativity() {
        // Left-associative
        assert_eq!(eval_c_expr("10 - 3 - 2").unwrap(), 5); // (10-3)-2
        assert_eq!(eval_c_expr("100 / 10 / 2").unwrap(), 5); // (100/10)/2
        // Right-associative ternary
        assert_eq!(eval_c_expr("1 ? 2 : 0 ? 3 : 4").unwrap(), 2);
        assert_eq!(eval_c_expr("0 ? 2 : 1 ? 3 : 4").unwrap(), 3);
    }

    #[test]
    fn test_expr_all_precedence_levels() {
        // Test each C precedence level
        assert_eq!(eval_c_expr("1 || 0").unwrap(), 1); // level 3
        assert_eq!(eval_c_expr("1 && 1").unwrap(), 1); // level 4
        assert_eq!(eval_c_expr("5 | 2").unwrap(), 7); // level 5
        assert_eq!(eval_c_expr("7 ^ 3").unwrap(), 4); // level 6
        assert_eq!(eval_c_expr("7 & 3").unwrap(), 3); // level 7
        assert_eq!(eval_c_expr("2 == 2").unwrap(), 1); // level 8
        assert_eq!(eval_c_expr("1 < 2").unwrap(), 1); // level 9
        assert_eq!(eval_c_expr("1 << 3").unwrap(), 8); // level 10
        assert_eq!(eval_c_expr("3 + 4").unwrap(), 7); // level 11
        assert_eq!(eval_c_expr("3 * 4").unwrap(), 12); // level 12
    }

    #[test]
    fn test_expr_mixed_precedence() {
        assert_eq!(eval_c_expr("1 || 0 && 0").unwrap(), 1); // && before ||
        assert_eq!(eval_c_expr("1 + 1 == 2").unwrap(), 1); // + before ==
        assert_eq!(eval_c_expr("1 + 2 < 4").unwrap(), 1); // + before <
        assert_eq!(eval_c_expr("1 + 2 * 3 + 4").unwrap(), 11);
    }

    #[test]
    fn test_expr_unary() {
        assert_eq!(eval_c_expr("-1 + 3").unwrap(), 2);
        assert_eq!(eval_c_expr("-1 & 2").unwrap(), 2);
        assert_eq!(eval_c_expr("-1 * 2").unwrap(), -2);
        assert_eq!(eval_c_expr("-5").unwrap(), -5);
        assert_eq!(eval_c_expr("!0").unwrap(), 1);
        assert_eq!(eval_c_expr("!1").unwrap(), 0);
        assert_eq!(eval_c_expr("~0").unwrap(), -1);
    }

    #[test]
    fn test_expr_ternary() {
        assert_eq!(eval_c_expr("1 ? 2 : 3").unwrap(), 2);
        assert_eq!(eval_c_expr("0 ? 2 : 3").unwrap(), 3);
        assert_eq!(eval_c_expr("1 + 1 ? 2 : 3").unwrap(), 2); // + before ?
    }
}
