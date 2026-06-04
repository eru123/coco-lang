use coco_span::Span;

// ============================================================
// Top-level
// ============================================================

#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Item {
    FnDecl(FnDecl),
    ClassDecl(ClassDecl),
    InterfaceDecl(InterfaceDecl),
    TraitDecl(TraitDecl),
    EnumDecl(EnumDecl),
    ConstDecl(ConstDecl),
    LetDecl(LetDecl),
    TypeAlias(TypeAlias),
    Import(Import),
    Export(Export),
    ExprStmt(ExprStmt),
    Stmt(Stmt),
}

// ============================================================
// Basic types
// ============================================================

#[derive(Debug, Clone)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub span: Span,
    pub name: Ident,
    pub type_ann: Option<Type>,
    pub default_value: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct TypeParam {
    pub span: Span,
    pub name: Ident,
    pub constraint: Option<Type>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Public,
    Private,
    Protected,
    Readonly,
    Static,
}

// ============================================================
// Function Declaration
// ============================================================

#[derive(Debug, Clone)]
pub struct FnDecl {
    pub span: Span,
    pub is_async: bool,
    pub name: Ident,
    pub type_params: Option<Vec<TypeParam>>,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: Block,
}

// ============================================================
// Class Declaration
// ============================================================

#[derive(Debug, Clone)]
pub struct ClassDecl {
    pub span: Span,
    pub name: Ident,
    pub type_params: Option<Vec<TypeParam>>,
    pub extends: Option<Type>,
    pub implements: Vec<Type>,
    pub members: Vec<ClassMember>,
}

#[derive(Debug, Clone)]
pub enum ClassMember {
    Constructor(Constructor),
    Method(Method),
    Property(Property),
    UseTrait(UseTrait),
}

#[derive(Debug, Clone)]
pub struct Constructor {
    pub span: Span,
    pub params: Vec<ConstructorParam>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct ConstructorParam {
    pub span: Span,
    pub modifiers: Vec<Modifier>,
    pub name: Ident,
    pub type_ann: Option<Type>,
    pub default_value: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct Method {
    pub span: Span,
    pub modifiers: Vec<Modifier>,
    pub is_async: bool,
    pub name: Ident,
    pub type_params: Option<Vec<TypeParam>>,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct Property {
    pub span: Span,
    pub modifiers: Vec<Modifier>,
    pub name: Ident,
    pub type_ann: Type,
    pub default_value: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct UseTrait {
    pub span: Span,
    pub traits: Vec<Ident>,
}

// ============================================================
// Interface Declaration
// ============================================================

#[derive(Debug, Clone)]
pub struct InterfaceDecl {
    pub span: Span,
    pub name: Ident,
    pub type_params: Option<Vec<TypeParam>>,
    pub extends: Option<Type>,
    pub members: Vec<InterfaceMember>,
}

#[derive(Debug, Clone)]
pub enum InterfaceMember {
    MethodSignature(MethodSignature),
    PropertySignature(PropertySignature),
}

#[derive(Debug, Clone)]
pub struct MethodSignature {
    pub span: Span,
    pub is_async: bool,
    pub name: Ident,
    pub type_params: Option<Vec<TypeParam>>,
    pub params: Vec<Param>,
    pub return_type: Type,
}

#[derive(Debug, Clone)]
pub struct PropertySignature {
    pub span: Span,
    pub name: Ident,
    pub type_ann: Type,
}

// ============================================================
// Trait Declaration
// ============================================================

#[derive(Debug, Clone)]
pub struct TraitDecl {
    pub span: Span,
    pub name: Ident,
    pub type_params: Option<Vec<TypeParam>>,
    pub members: Vec<TraitMember>,
}

#[derive(Debug, Clone)]
pub enum TraitMember {
    Method(Method),
    MethodSignature(MethodSignature),
    Property(Property),
}

// ============================================================
// Enum Declaration
// ============================================================

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub span: Span,
    pub name: Ident,
    pub backing_type: Option<Type>,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub span: Span,
    pub name: Ident,
    pub fields: Option<Vec<Type>>,
    pub value: Option<Expr>,
}

// ============================================================
// Const / Let Declarations
// ============================================================

#[derive(Debug, Clone)]
pub struct ConstDecl {
    pub span: Span,
    pub name: Ident,
    pub type_ann: Option<Type>,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct LetDecl {
    pub span: Span,
    pub name: Ident,
    pub type_ann: Option<Type>,
    pub value: Option<Expr>,
}

// ============================================================
// Type Alias
// ============================================================

#[derive(Debug, Clone)]
pub struct TypeAlias {
    pub span: Span,
    pub name: Ident,
    pub type_params: Option<Vec<TypeParam>>,
    pub target: Type,
}

// ============================================================
// Import / Export
// ============================================================

#[derive(Debug, Clone)]
pub struct Import {
    pub span: Span,
    pub items: ImportItems,
    pub source: String,
}

#[derive(Debug, Clone)]
pub enum ImportItems {
    Named(Vec<Ident>),
    Namespace(Ident),
}

#[derive(Debug, Clone)]
pub struct Export {
    pub span: Span,
    pub item: Box<Item>,
}

// ============================================================
// Statements
// ============================================================

#[derive(Debug, Clone)]
pub enum Stmt {
    Expr(ExprStmt),
    If(IfStmt),
    For(ForStmt),
    While(WhileStmt),
    DoWhile(DoWhileStmt),
    Loop(LoopStmt),
    Return(ReturnStmt),
    Throw(ThrowStmt),
    Try(TryStmt),
    Break(BreakStmt),
    Continue(ContinueStmt),
    Parallel(ParallelStmt),
    Coro(CoroStmt),
    Select(SelectStmt),
    Unsafe(UnsafeStmt),
    Synchronized(SynchronizedStmt),
}

#[derive(Debug, Clone)]
pub struct ExprStmt {
    pub span: Span,
    pub expr: Expr,
}

#[derive(Debug, Clone)]
pub struct IfStmt {
    pub span: Span,
    pub condition: Expr,
    pub then_block: Block,
    pub else_ifs: Vec<ElseIf>,
    pub else_block: Option<Block>,
}

#[derive(Debug, Clone)]
pub struct ElseIf {
    pub span: Span,
    pub condition: Expr,
    pub block: Block,
}

#[derive(Debug, Clone)]
pub struct ForStmt {
    pub span: Span,
    pub pattern: Ident,
    pub iterable: Expr,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct WhileStmt {
    pub span: Span,
    pub condition: Expr,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct DoWhileStmt {
    pub span: Span,
    pub body: Block,
    pub condition: Expr,
}

#[derive(Debug, Clone)]
pub struct LoopStmt {
    pub span: Span,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct ReturnStmt {
    pub span: Span,
    pub value: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct ThrowStmt {
    pub span: Span,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct TryStmt {
    pub span: Span,
    pub body: Block,
    pub catches: Vec<CatchClause>,
    pub finally: Option<Block>,
}

#[derive(Debug, Clone)]
pub struct CatchClause {
    pub span: Span,
    pub param: Ident,
    pub type_ann: Option<Type>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct BreakStmt {
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ContinueStmt {
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ParallelStmt {
    pub span: Span,
    pub runs: Vec<RunClause>,
}

#[derive(Debug, Clone)]
pub struct RunClause {
    pub span: Span,
    pub expr: Expr,
}

#[derive(Debug, Clone)]
pub struct CoroStmt {
    pub span: Span,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct SelectStmt {
    pub span: Span,
    pub cases: Vec<CaseClause>,
}

#[derive(Debug, Clone)]
pub struct CaseClause {
    pub span: Span,
    pub pattern: Ident,
    pub expr: Expr,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct UnsafeStmt {
    pub span: Span,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct SynchronizedStmt {
    pub span: Span,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub span: Span,
    pub stmts: Vec<Stmt>,
}

// ============================================================
// Expressions
// ============================================================

#[derive(Debug, Clone)]
pub enum Expr {
    Literal(Literal),
    Ident(Ident),
    Binary(Box<BinaryExpr>),
    Unary(Box<UnaryExpr>),
    Call(Box<CallExpr>),
    Index(Box<IndexExpr>),
    Member(Box<MemberExpr>),
    Match(Box<MatchExpr>),
    Lambda(Box<Lambda>),
    Array(ArrayLiteral),
    Object(ObjectLiteral),
    This(Span),
    Dollar(Span),
    DollarDollar(Span),
    New(Box<NewExpr>),
    Ternary(Box<TernaryExpr>),
    NullCoalesce(Box<NullCoalesceExpr>),
    Elvis(Box<ElvisExpr>),
    Pipe(Box<PipeExpr>),
    Assignment(Box<AssignmentExpr>),
    Postfix(Box<PostfixExpr>),
    Group(Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum Literal {
    Int(i64, Span),
    Float(f64, Span),
    String(String, Span),
    Char(char, Span),
    Bool(bool, Span),
    Null(Span),
}

#[derive(Debug, Clone)]
pub struct BinaryExpr {
    pub span: Span,
    pub left: Expr,
    pub op: BinaryOp,
    pub right: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Spaceship,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    PipeRight,
    PipeLeft,
    NullCoalesce,
    Elvis,
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
    PowAssign,
    ShlAssign,
    ShrAssign,
    BitAndAssign,
    BitOrAssign,
    BitXorAssign,
    Range,
    RangeInclusive,
}

#[derive(Debug, Clone)]
pub struct UnaryExpr {
    pub span: Span,
    pub op: UnaryOp,
    pub expr: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    BitNot,
    Neg,
    Typeof,
    New,
    Await,
    Lazy,
}

#[derive(Debug, Clone)]
pub struct CallExpr {
    pub span: Span,
    pub callee: Expr,
    pub args: Vec<Argument>,
}

#[derive(Debug, Clone)]
pub struct Argument {
    pub span: Span,
    pub name: Option<Ident>,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct IndexExpr {
    pub span: Span,
    pub object: Expr,
    pub index: Expr,
}

#[derive(Debug, Clone)]
pub struct MemberExpr {
    pub span: Span,
    pub object: Expr,
    pub property: Ident,
    pub optional: bool,
}

#[derive(Debug, Clone)]
pub struct MatchExpr {
    pub span: Span,
    pub scrutinee: Expr,
    pub arms: Vec<MatchArm>,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub span: Span,
    pub pattern: Pattern,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Literal(Literal),
    Ident(Ident),
    IsType(Type),
    Wildcard(Span),
}

#[derive(Debug, Clone)]
pub struct Lambda {
    pub span: Span,
    pub is_async: bool,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: LambdaBody,
}

#[derive(Debug, Clone)]
pub enum LambdaBody {
    Expr(Expr),
    Block(Block),
}

#[derive(Debug, Clone)]
pub struct ArrayLiteral {
    pub span: Span,
    pub elements: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub struct ObjectLiteral {
    pub span: Span,
    pub fields: Vec<ObjectField>,
}

#[derive(Debug, Clone)]
pub struct ObjectField {
    pub span: Span,
    pub key: ObjectKey,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub enum ObjectKey {
    Ident(Ident),
    String(String, Span),
}

#[derive(Debug, Clone)]
pub struct NewExpr {
    pub span: Span,
    pub type_name: Ident,
    pub args: Vec<Argument>,
}

#[derive(Debug, Clone)]
pub struct TernaryExpr {
    pub span: Span,
    pub condition: Expr,
    pub then_expr: Expr,
    pub else_expr: Expr,
}

#[derive(Debug, Clone)]
pub struct NullCoalesceExpr {
    pub span: Span,
    pub left: Expr,
    pub right: Expr,
}

#[derive(Debug, Clone)]
pub struct ElvisExpr {
    pub span: Span,
    pub left: Expr,
    pub right: Expr,
}

#[derive(Debug, Clone)]
pub struct PipeExpr {
    pub span: Span,
    pub left: Expr,
    pub op: PipeOp,
    pub right: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeOp {
    PipeRight,
    PipeLeft,
}

#[derive(Debug, Clone)]
pub struct AssignmentExpr {
    pub span: Span,
    pub target: Expr,
    pub op: AssignmentOp,
    pub value: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
    PowAssign,
    ShlAssign,
    ShrAssign,
    BitAndAssign,
    BitOrAssign,
    BitXorAssign,
}

#[derive(Debug, Clone)]
pub struct PostfixExpr {
    pub span: Span,
    pub object: Expr,
    pub op: PostfixOp,
}

#[derive(Debug, Clone)]
pub enum PostfixOp {
    Dot(Ident),
    QuestionDot(Ident),
    Index(Expr),
    Call(Vec<Argument>),
    Bang,
    Question,
}

// ============================================================
// Types
// ============================================================

#[derive(Debug, Clone)]
pub enum Type {
    Primitive(PrimitiveType, Span),
    Named(NamedType),
    Union(UnionType),
    Intersection(IntersectionType),
    List(ListType),
    Map(MapType),
    Tuple(TupleType),
    Result(ResultType),
    Function(FunctionType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveType {
    Int,
    Uint,
    Float,
    Bool,
    String,
    Char,
    Byte,
    Null,
    Void,
    Never,
    Mixed,
}

#[derive(Debug, Clone)]
pub struct NamedType {
    pub span: Span,
    pub name: Ident,
    pub type_args: Option<Vec<Type>>,
}

#[derive(Debug, Clone)]
pub struct UnionType {
    pub span: Span,
    pub types: Vec<Type>,
}

#[derive(Debug, Clone)]
pub struct IntersectionType {
    pub span: Span,
    pub types: Vec<Type>,
}

#[derive(Debug, Clone)]
pub struct ListType {
    pub span: Span,
    pub element_type: Box<Type>,
}

#[derive(Debug, Clone)]
pub struct MapType {
    pub span: Span,
    pub key_type: Box<Type>,
    pub value_type: Box<Type>,
}

#[derive(Debug, Clone)]
pub struct TupleType {
    pub span: Span,
    pub element_types: Vec<Type>,
}

#[derive(Debug, Clone)]
pub struct ResultType {
    pub span: Span,
    pub ok_type: Box<Type>,
    pub err_type: Box<Type>,
}

#[derive(Debug, Clone)]
pub struct FunctionType {
    pub span: Span,
    pub param_types: Vec<Type>,
    pub return_type: Box<Type>,
}

// ============================================================
// Span helper methods
// ============================================================

impl Type {
    pub fn span(&self) -> Span {
        match self {
            Type::Primitive(_, s) => *s,
            Type::Named(n) => n.span,
            Type::Union(u) => u.span,
            Type::Intersection(i) => i.span,
            Type::List(l) => l.span,
            Type::Map(m) => m.span,
            Type::Tuple(t) => t.span,
            Type::Result(r) => r.span,
            Type::Function(f) => f.span,
        }
    }
}

impl Expr {
    pub fn span_start(&self) -> usize {
        self.span().start
    }

    pub fn span_end(&self) -> usize {
        self.span().end
    }

    pub fn span(&self) -> Span {
        match self {
            Expr::Literal(l) => l.span(),
            Expr::Ident(i) => i.span,
            Expr::Binary(e) => e.span,
            Expr::Unary(e) => e.span,
            Expr::Call(e) => e.span,
            Expr::Index(e) => e.span,
            Expr::Member(e) => e.span,
            Expr::Match(e) => e.span,
            Expr::Lambda(e) => e.span,
            Expr::Array(a) => a.span,
            Expr::Object(o) => o.span,
            Expr::This(s) => *s,
            Expr::Dollar(s) => *s,
            Expr::DollarDollar(s) => *s,
            Expr::New(e) => e.span,
            Expr::Ternary(e) => e.span,
            Expr::NullCoalesce(e) => e.span,
            Expr::Elvis(e) => e.span,
            Expr::Pipe(e) => e.span,
            Expr::Assignment(e) => e.span,
            Expr::Postfix(e) => e.span,
            Expr::Group(e) => e.span(),
        }
    }
}

impl Literal {
    pub fn span(&self) -> Span {
        match self {
            Literal::Int(_, s) => *s,
            Literal::Float(_, s) => *s,
            Literal::String(_, s) => *s,
            Literal::Char(_, s) => *s,
            Literal::Bool(_, s) => *s,
            Literal::Null(s) => *s,
        }
    }
}
