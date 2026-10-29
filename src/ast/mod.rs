//! A small AST for graphs (Blueprint exec graphs, anim graphs, material graphs) and a
//! pseudocode printer for it.

pub mod bp;
pub mod material;

use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Literal, already formatted (`1.5`, `"text"`, `true`, `/Game/Foo.Foo`).
    Lit(String),
    /// Variable, parameter or temporary.
    Var(String),
    SelfRef,
    /// `object.member`
    Member(Box<Expr>, String),
    /// `target.func(name=value, ...)`; positional when the name is empty.
    Call { target: Option<Box<Expr>>, func: String, args: Vec<(String, Expr)> },
    Binary(String, Box<Expr>, Box<Expr>),
    Unary(String, Box<Expr>),
    /// `cast<T>(e)`
    Cast(String, Box<Expr>),
    /// `a[i]`
    Index(Box<Expr>, Box<Expr>),
    /// `[a, b]`
    List(Vec<Expr>),
    /// `Type{field=value, ...}`
    Struct(String, Vec<(String, Expr)>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Expr(Expr),
    /// `let a, b = expr` (outputs captured from an impure call)
    Let(Vec<String>, Expr),
    Assign(Expr, Expr),
    If { cond: Expr, then: Vec<Stmt>, els: Vec<Stmt> },
    /// `if (let name = cast<T>(obj)) {..} else {..}`
    IfLet { name: String, value: Expr, then: Vec<Stmt>, els: Vec<Stmt> },
    Switch { value: Expr, cases: Vec<(String, Vec<Stmt>)> },
    /// `for (header) { body }` followed by `completed` statements
    Loop { header: String, body: Vec<Stmt>, completed: Vec<Stmt> },
    /// Generic node with several exec outputs: `head { on A: ...  on B: ... }`
    Handlers { head: Expr, handlers: Vec<(String, Vec<Stmt>)> },
    Await(Expr),
    Return(Vec<(String, Expr)>),
    Goto(String),
    Break,
    Comment(String),
    /// Raw line of pseudocode.
    Line(String),
    /// `head { body }`
    Block { head: String, body: Vec<Stmt> },
}

/// A function-like unit: event handler, function, macro, anim graph...
#[derive(Debug, Clone, Default)]
pub struct Function {
    /// "event", "function", "macro", "animgraph", "material", ...
    pub kind: String,
    pub name: String,
    pub params: Vec<(String, String)>,
    pub returns: Vec<(String, String)>,
    pub tags: Vec<String>,
    pub locals: Vec<(String, String)>,
    pub body: Vec<Stmt>,
}

/// Shared labelled block (exec code reachable from several places).
#[derive(Debug, Clone)]
pub struct Block {
    pub label: String,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Default)]
pub struct Program {
    pub functions: Vec<Function>,
    pub blocks: Vec<Block>,
}

// ---------------------------------------------------------------------------------------------
// Printing

fn prec(op: &str) -> u8 {
    match op {
        "||" => 1,
        "&&" => 2,
        "|" => 3,
        "^" => 4,
        "&" => 5,
        "==" | "!=" => 6,
        "<" | ">" | "<=" | ">=" => 7,
        "<<" | ">>" => 8,
        "+" | "-" => 9,
        "*" | "/" | "%" => 10,
        _ => 11,
    }
}

impl Expr {
    pub fn lit(s: impl Into<String>) -> Expr {
        Expr::Lit(s.into())
    }
    pub fn var(s: impl Into<String>) -> Expr {
        Expr::Var(s.into())
    }
    pub fn call(func: impl Into<String>, args: Vec<(String, Expr)>) -> Expr {
        Expr::Call { target: None, func: func.into(), args }
    }

    fn write(&self, out: &mut String, parent_prec: u8) {
        match self {
            Expr::Lit(s) | Expr::Var(s) => out.push_str(s),
            Expr::SelfRef => out.push_str("self"),
            Expr::Member(o, m) => {
                o.write(out, 12);
                out.push('.');
                out.push_str(&ident(m));
            }
            Expr::Call { target, func, args } => {
                if let Some(t) = target {
                    t.write(out, 12);
                    out.push('.');
                }
                out.push_str(func);
                out.push('(');
                for (i, (n, a)) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    if !n.is_empty() {
                        out.push_str(&ident(n));
                        out.push_str(": ");
                    }
                    a.write(out, 0);
                }
                out.push(')');
            }
            Expr::Binary(op, a, b) => {
                let p = prec(op);
                if p < parent_prec {
                    out.push('(');
                }
                a.write(out, p);
                let _ = write!(out, " {op} ");
                b.write(out, p + 1);
                if p < parent_prec {
                    out.push(')');
                }
            }
            Expr::Unary(op, a) => {
                out.push_str(op);
                a.write(out, 12);
            }
            Expr::Cast(t, e) => {
                let _ = write!(out, "cast<{t}>(");
                e.write(out, 0);
                out.push(')');
            }
            Expr::Index(a, i) => {
                a.write(out, 12);
                out.push('[');
                i.write(out, 0);
                out.push(']');
            }
            Expr::List(v) => {
                out.push('[');
                for (i, e) in v.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    e.write(out, 0);
                }
                out.push(']');
            }
            Expr::Struct(t, fields) => {
                out.push_str(t);
                out.push('{');
                for (i, (n, e)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&ident(n));
                    out.push_str(": ");
                    e.write(out, 0);
                }
                out.push('}');
            }
        }
    }

    pub fn to_code(&self) -> String {
        let mut s = String::new();
        self.write(&mut s, 0);
        s
    }

    /// Number of nodes (used to decide inlining).
    pub fn size(&self) -> usize {
        match self {
            Expr::Lit(_) | Expr::Var(_) | Expr::SelfRef => 1,
            Expr::Member(o, _) => 1 + o.size(),
            Expr::Call { target, args, .. } => 1 + target.as_ref().map(|t| t.size()).unwrap_or(0) + args.iter().map(|(_, a)| a.size()).sum::<usize>(),
            Expr::Binary(_, a, b) => 1 + a.size() + b.size(),
            Expr::Unary(_, a) | Expr::Cast(_, a) => 1 + a.size(),
            Expr::Index(a, b) => 1 + a.size() + b.size(),
            Expr::List(v) => 1 + v.iter().map(|e| e.size()).sum::<usize>(),
            Expr::Struct(_, f) => 1 + f.iter().map(|(_, e)| e.size()).sum::<usize>(),
        }
    }
}

/// Makes a pin / variable name usable as an identifier (spaces and punctuation removed).
pub fn ident(name: &str) -> String {
    if name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.is_empty() {
        return name.to_string();
    }
    let mut s = String::new();
    let mut upper = false;
    for c in name.chars() {
        if c.is_alphanumeric() || c == '_' {
            if upper {
                s.extend(c.to_uppercase());
            } else {
                s.push(c);
            }
            upper = false;
        } else {
            upper = !s.is_empty();
        }
    }
    if s.is_empty() { "_".into() } else { s }
}

pub struct Printer {
    pub out: String,
    pub indent: usize,
}

impl Printer {
    pub fn new(indent: usize) -> Self {
        Self { out: String::new(), indent }
    }

    fn line(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn block(&mut self, head: &str, body: &[Stmt]) {
        self.line(&format!("{head} {{"));
        self.indent += 1;
        self.stmts(body);
        self.indent -= 1;
    }

    pub fn stmts(&mut self, body: &[Stmt]) {
        for s in body {
            self.stmt(s);
        }
    }

    fn else_chain(&mut self, els: &[Stmt]) {
        if els.is_empty() {
            self.line("}");
            return;
        }
        // `else if` for a nested single if
        if let [Stmt::If { cond, then, els: e2 }] = els {
            let head = format!("}} else if ({}) {{", cond.to_code());
            self.line(&head);
            self.indent += 1;
            self.stmts(then);
            self.indent -= 1;
            self.else_chain(e2);
            return;
        }
        self.line("} else {");
        self.indent += 1;
        self.stmts(els);
        self.indent -= 1;
        self.line("}");
    }

    pub fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Expr(e) => self.line(&e.to_code()),
            Stmt::Let(names, e) => {
                let names: Vec<String> = names.iter().map(|n| ident(n)).collect();
                self.line(&format!("let {} = {}", names.join(", "), e.to_code()))
            }
            Stmt::Assign(a, b) => self.line(&format!("{} = {}", a.to_code(), b.to_code())),
            Stmt::If { cond, then, els } if then.is_empty() && els.is_empty() => {
                self.line(&format!("if ({}) {{}}", cond.to_code()));
            }
            Stmt::If { cond, then, els } => {
                self.block(&format!("if ({})", cond.to_code()), then);
                self.else_chain(els);
            }
            Stmt::IfLet { name, value, then, els } => {
                self.block(&format!("if (let {} = {})", ident(name), value.to_code()), then);
                self.else_chain(els);
            }
            Stmt::Switch { value, cases } => {
                self.line(&format!("switch ({}) {{", value.to_code()));
                self.indent += 1;
                for (label, body) in cases {
                    self.line(&format!("case {label}:"));
                    self.indent += 1;
                    if body.is_empty() {
                        self.line("pass");
                    }
                    self.stmts(body);
                    self.indent -= 1;
                }
                self.indent -= 1;
                self.line("}");
            }
            Stmt::Loop { header, body, completed } => {
                self.block(header, body);
                self.line("}");
                self.stmts(completed);
            }
            Stmt::Line(l) => self.line(l),
            Stmt::Block { head, body } => {
                if body.is_empty() {
                    self.line(&format!("{head} {{}}"));
                } else {
                    self.block(head, body);
                    self.line("}");
                }
            }
            Stmt::Handlers { head, handlers } => {
                self.line(&format!("{} {{", head.to_code()));
                self.indent += 1;
                for (name, body) in handlers {
                    self.line(&format!("on {}:", ident(name)));
                    self.indent += 1;
                    self.stmts(body);
                    self.indent -= 1;
                }
                self.indent -= 1;
                self.line("}");
            }
            Stmt::Await(e) => self.line(&format!("await {}", e.to_code())),
            Stmt::Return(v) => {
                if v.is_empty() {
                    self.line("return");
                } else if v.len() == 1 && v[0].0 == "ReturnValue" {
                    self.line(&format!("return {}", v[0].1.to_code()));
                } else {
                    let parts: Vec<String> = v.iter().map(|(n, e)| format!("{}: {}", ident(n), e.to_code())).collect();
                    self.line(&format!("return ({})", parts.join(", ")));
                }
            }
            Stmt::Goto(l) => self.line(&format!("goto {l}")),
            Stmt::Break => self.line("break"),
            Stmt::Comment(c) => {
                for l in c.lines() {
                    self.line(&format!("// {l}"));
                }
            }
        }
    }

    pub fn function(&mut self, f: &Function) {
        let params: Vec<String> = f.params.iter().map(|(n, t)| if t.is_empty() { ident(n) } else { format!("{}: {t}", ident(n)) }).collect();
        let mut head = if f.kind == "unconnected" {
            format!("unconnected code (starts at {})", quote_name(&f.name))
        } else {
            format!("{} {}({})", f.kind, quote_name(&f.name), params.join(", "))
        };
        if !f.returns.is_empty() {
            let r: Vec<String> = f.returns.iter().map(|(n, t)| format!("{}: {t}", ident(n))).collect();
            head.push_str(&format!(" -> ({})", r.join(", ")));
        }
        if !f.tags.is_empty() {
            head.push_str(&format!(" [{}]", f.tags.join(", ")));
        }
        self.line(&format!("{head} {{"));
        self.indent += 1;
        for (n, t) in &f.locals {
            self.line(&format!("local {}: {t}", ident(n)));
        }
        self.stmts(&f.body);
        self.indent -= 1;
        self.line("}");
    }

    pub fn program(&mut self, p: &Program) {
        for (i, f) in p.functions.iter().enumerate() {
            if i > 0 {
                self.out.push('\n');
            }
            self.function(f);
        }
        for b in &p.blocks {
            self.out.push('\n');
            self.line(&format!("{}: {{", b.label));
            self.indent += 1;
            self.stmts(&b.body);
            self.indent -= 1;
            self.line("}");
        }
    }
}

fn quote_name(n: &str) -> String {
    if !n.is_empty() && n.chars().all(|c| c.is_alphanumeric() || c == '_') { n.to_string() } else { format!("\"{n}\"") }
}

/// Removes trailing zeros from numbers in UE default value strings: `0.500000` -> `0.5`,
/// `(R=0.000000,G=1.000000)` -> `(R=0,G=1)`.
pub fn tidy_numbers(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let starts_number = c.is_ascii_digit() && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_'));
        if starts_number {
            let st = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i + 1 < b.len() && b[i] == b'.' && b[i + 1].is_ascii_digit() {
                let dot = i;
                i += 1;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                let next_alpha = i < b.len() && (b[i].is_ascii_alphabetic() || b[i] == b'_');
                if next_alpha {
                    out.extend_from_slice(&b[st..i]);
                    continue;
                }
                let frac = s[dot + 1..i].trim_end_matches('0');
                out.extend_from_slice(&b[st..dot]);
                if !frac.is_empty() {
                    out.push(b'.');
                    out.extend_from_slice(frac.as_bytes());
                }
                continue;
            }
            out.extend_from_slice(&b[st..i]);
            continue;
        }
        out.push(c);
        i += 1;
    }
    // only ASCII digits/dots are removed, so the result is still valid UTF-8
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(tidy_numbers("0.500000"), "0.5");
        assert_eq!(tidy_numbers("(R=0.000000,G=1.000000)"), "(R=0,G=1)");
        assert_eq!(tidy_numbers("-90.000000"), "-90");
        assert_eq!(tidy_numbers("Vec3f12.5"), "Vec3f12.5");
    }

    #[test]
    fn precedence() {
        let e = Expr::Binary(
            "*".into(),
            Box::new(Expr::Binary("+".into(), Box::new(Expr::var("a")), Box::new(Expr::var("b")))),
            Box::new(Expr::var("c")),
        );
        assert_eq!(e.to_code(), "(a + b) * c");
        assert_eq!(ident("Array Element"), "ArrayElement");
    }
}
