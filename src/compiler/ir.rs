use std::{cell::RefCell, collections::HashMap, fmt, vec::IntoIter};

use crate::{
    exeptions::GenericError,
    generic_error,
    instructions::{Bytecode, Instr, Opcode},
    value::{List, Value},
};

#[derive(Debug)]
pub enum Operation {
    Pop,
    Dup,
    Dup2,
    Swap,
    Over,

    Add,
    Minus,
    Mul,
    Div,
    Mod,

    Eq,
    Neq,
    Gt,
    Gte,
    Lte,
    Lt,

    Land,
    Lor,

    Shl,
    Shr,
    Bitand,
    Bitor,
}

#[derive(Debug)]
pub enum BuildinOp {
    IdxGet,
    IdxSet,
    Len,
    Println,
    Print,
    Debug,
    FuncCall,
}

impl From<&BuildinOp> for usize {
    fn from(value: &BuildinOp) -> Self {
        match value {
            BuildinOp::IdxGet => 0,
            BuildinOp::IdxSet => 1,
            BuildinOp::Len => 2,
            BuildinOp::Println => 3,
            BuildinOp::Print => 4,
            BuildinOp::Debug => 5,
            BuildinOp::FuncCall => 12,
        }
    }
}

#[derive(Debug)]
pub struct IfExpr {
    pub cond: Expressions,
    pub if_branch: Expressions,
    pub else_branch: Option<Expressions>,
}

#[derive(Debug)]
pub struct WhileExpr {
    pub cond: Expressions,
    pub while_block: Expressions,
}

#[derive(Debug)]
pub struct VarExpr {
    pub name: String,
    pub value: Expressions,
}

#[derive(Debug)]
pub struct PeekExpr {
    pub names: Vec<String>,
    pub body: Expressions,
}

#[derive(Debug)]
pub struct ListLiteral {
    pub value: List,
}

#[derive(Debug)]
pub enum Expr {
    Op(Box<Operation>),
    Buildin(Box<BuildinOp>),
    IntExpr(Box<String>),
    StrExpr(Box<String>),
    ListExpr(Box<ListLiteral>),
    IdentExpr(Box<String>),
    If(Box<IfExpr>),
    Whlie(Box<WhileExpr>),
    Var(Box<VarExpr>),
    Peek(Box<PeekExpr>),
    Assigin(Box<String>),
    Set,
    IndexExpr,
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Op(_) => write!(f, "Op"),
            Expr::Buildin(_) => write!(f, "Buildin"),
            Expr::IntExpr(_) => write!(f, "IntExpr"),
            Expr::StrExpr(_) => write!(f, "StrExpr"),
            Expr::If(_) => write!(f, "If"),
            Expr::Whlie(_) => write!(f, "Whlie"),
            Expr::Var(_) => write!(f, "Var"),
            Expr::Peek(_) => write!(f, "Peek"),
            Expr::ListExpr(_) => write!(f, "ListExpr"),
            Expr::IdentExpr(_) => write!(f, "Identifier"),
            Expr::Assigin(_) => write!(f, "Assigin"),
            Expr::Set => write!(f, "Set"),
            Expr::IndexExpr => write!(f, "IndexExpr"),
        }
    }
}

type Expressions = Vec<Expr>;
pub struct Program {
    pub exprs: Expressions,
}

impl IntoIterator for Program {
    type Item = Expr;
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.exprs.into_iter()
    }
}

pub struct IrParser {
    program: IntoIter<Expr>,
    instrs: Vec<Instr>,
    consts: Vec<Value>,
    var_def: HashMap<String, usize>,
    local_def: HashMap<String, usize>,
    var_count: usize,
}

impl IrParser {
    pub fn new(program: Program) -> Self {
        Self {
            program: program.into_iter(),
            instrs: Vec::new(),
            consts: Vec::new(),
            var_def: HashMap::new(),
            var_count: 0,
            local_def: HashMap::new(),
        }
    }

    pub fn parse(&mut self) -> Result<Bytecode, GenericError> {
        while let Some(expr) = self.program.next() {
            self.expr(expr)?;
        }

        Ok(Bytecode {
            program: self.instrs.clone(),
            consts: self.consts.clone(),
        })
    }

    fn expr(&mut self, expr: Expr) -> Result<(), GenericError> {
        Ok(match expr {
            Expr::If(v) => self.if_expr(*v)?,
            Expr::Whlie(v) => self.while_expr(*v)?,
            Expr::Var(v) => self.var_expr(*v)?,
            Expr::Peek(v) => self.peek_expr(*v)?,
            _ => self.simple_expr(expr)?,
        })
    }

    fn peek_expr(&mut self, expr: PeekExpr) -> Result<(), GenericError> {
        let names_len = expr.names.len();
        let mut shadow_names: Vec<(String, usize)> = vec![];
        self.instrs.push(Instr::new(Opcode::Bind, Some(names_len)));
        for e in expr.names.iter().rev() {
            let local_count = self.local_def.len();
            if self.var_def.get(e).is_some() { generic_error!("Peek {} is already a variable name", e) };
            if let Some(shadow) = self.local_def.insert(e.to_string(), local_count) {
                shadow_names.push((e.to_string(), shadow));
            }
        }
        for e in expr.body.into_iter() {
            self.expr(e)?
        }
        self.instrs
            .push(Instr::new(Opcode::Unbind, Some(names_len)));
        for e in expr.names.iter() {
            self.local_def.remove(e);
        }
        if shadow_names.len() != 0 {
            for (name, pos) in shadow_names.into_iter() {
                self.local_def.insert(name, pos);
            }
        }
        Ok(())
    }

    fn var_expr(&mut self, expr: VarExpr) -> Result<(), GenericError> {
        let var_ptr = self.var_count;
        if self.local_def.get(&expr.name).is_some() { generic_error!("Variable {} is already a peek name", expr.name) };
        self.var_count += 1;
        self.var_def.insert(expr.name.clone(), var_ptr);
        for e in expr.value.into_iter() {
            self.simple_expr(e)?;
        }
        self.instrs
            .push(Instr::new(Opcode::GlobalStore, Some(var_ptr)));
        Ok(())
    }

    fn while_expr(&mut self, expr: WhileExpr) -> Result<(), GenericError> {
        let whileaddrs = self.instrs.len();
        for e in expr.cond.into_iter() {
            self.expr(e)?
        }
        let ifaddrs = self.instrs.len();
        self.instrs.push(Instr::new(Opcode::JmpIf, None));
        for e in expr.while_block.into_iter() {
            self.expr(e)?
        }
        self.instrs.push(Instr::new(Opcode::Jmp, Some(whileaddrs)));
        let curr_len = self.instrs.len();
        let elem = unsafe { self.instrs.get_unchecked_mut(ifaddrs) };
        *elem = Instr::new(Opcode::JmpIf, Some(curr_len));
        Ok(())
    }

    fn if_expr(&mut self, expr: IfExpr) -> Result<(), GenericError> {
        for e in expr.cond.into_iter() {
            self.expr(e)?
        }
        let offset = self.instrs.len();
        self.instrs.push(Instr::new(Opcode::JmpIf, None));
        for e in expr.if_branch.into_iter() {
            self.expr(e)?
        }
        if let Some(vec) = expr.else_branch {
            let offset2 = self.instrs.len();
            self.instrs.push(Instr::new(Opcode::Jmp, None));
            let elem = unsafe { self.instrs.get_unchecked_mut(offset) };
            *elem = Instr::new(Opcode::JmpIf, Some(offset2 + 1));
            for e in vec.into_iter() {
                self.expr(e)?
            }
            let curr_len = self.instrs.len();
            let elem = unsafe { self.instrs.get_unchecked_mut(offset2) };
            *elem = Instr::new(Opcode::Jmp, Some(curr_len));
        } else {
            let curr_len = self.instrs.len();
            let elem = unsafe { self.instrs.get_unchecked_mut(offset) };
            *elem = Instr::new(Opcode::JmpIf, Some(curr_len));
        }
        Ok(())
    }

    fn simple_expr(&mut self, expr: Expr) -> Result<(), GenericError> {
        match expr {
            Expr::IntExpr(v) => {
                self.consts.push(Value::Int64(v.parse().unwrap()));
                self.instrs
                    .push(Instr::new(Opcode::Const, Some(self.consts.len() - 1)));
            }
            Expr::StrExpr(v) => {
                self.consts.push(Value::Str(v.to_string()));
                self.instrs
                    .push(Instr::new(Opcode::Const, Some(self.consts.len() - 1)));
            }
            Expr::ListExpr(v) => {
                self.consts.push(Value::List(RefCell::new(v.value.clone())));
                self.instrs
                    .push(Instr::new(Opcode::Const, Some(self.consts.len() - 1)));
            }
            Expr::Op(v) => {
                self.instrs.push(Instr::new(Opcode::from(v.as_ref()), None));
            }
            Expr::Buildin(v) => {
                self.instrs
                    .push(Instr::new(Opcode::Buildin, Some(usize::from(v.as_ref()))));
            }
            Expr::IdentExpr(val) => {
                if let Some(v) = self.local_def.get(val.as_ref()) {
                    self.instrs.push(Instr::new(Opcode::PushBind, Some(*v)));
                } else if let Some(v) = self.var_def.get(val.as_ref()) {
                    self.instrs.push(Instr::new(Opcode::GlobalLoad, Some(*v)));
                } else {
                    generic_error!("{} is not defined", val.to_string())
                }
            }
            Expr::Assigin(val) => {
                if let Some(v) = self.var_def.get(val.as_ref()) {
                    self.instrs.push(Instr::new(Opcode::GlobalStore, Some(*v)));
                } else {
                    let var_ptr = self.var_count;
                    self.var_count += 1;
                    self.var_def.insert(val.to_string(), var_ptr);
                    self.instrs
                        .push(Instr::new(Opcode::GlobalStore, Some(var_ptr)));
                }
            }
            e => generic_error!("{} is not simple expression", e),
        }
        Ok(())
    }
}