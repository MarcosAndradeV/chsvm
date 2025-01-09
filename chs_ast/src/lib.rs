use std::collections::BTreeMap;

use chs_lexer::{Lexer, Token, TokenKind};
use chs_types::CHSType;
use chs_util::{chs_error, CHSError, Loc};
use nodes::{
    Binop, Call, ConstExpression, Expression, Function, Module, Operator, Precedence, Unop, VarDecl
};

pub mod nodes;

/// [Token] -> [Module]
#[derive(Default)]
pub struct Parser {
    lexer: Lexer,
    peeked: Option<Token>,
    module: Module,
}

impl Parser {
    pub fn new(lexer: Lexer) -> Self {
        let modname = lexer
            .get_filename()
            .with_extension("")
            .to_string_lossy()
            .to_string()
            .replace("/", ".");
        Self {
            lexer,
            module: Module {
                name: modname,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn next(&mut self) -> Token {
        loop {
            let token = self
                .peeked
                .take()
                .unwrap_or_else(|| self.lexer.next_token());
            if token.kind == TokenKind::Comment {
                continue;
            }
            return token;
        }
    }

    fn expect_kind(&mut self, kind: TokenKind) -> Result<Token, CHSError> {
        let token = self.next();
        if token.kind != kind {
            chs_error!(
                "{} Unexpected token '{}' of '{}', Expect: {}",
                token.loc,
                token.kind,
                token.value,
                kind
            )
        }
        Ok(token)
    }

    fn peek(&mut self) -> &Token {
        if self.peeked.is_none() {
            self.peeked = Some(self.next());
        }
        self.peeked.as_ref().unwrap()
    }

    pub fn parse(mut self) -> Result<Module, CHSError> {
        use chs_lexer::TokenKind::*;
        loop {
            let token = self.next();
            if token.kind.is_eof() {
                break;
            }
            if token.kind == Invalid {
                chs_error!("{} Invalid token '{}'", token.loc, token.value);
            }
            match token.kind {
                Keyword if token.val_eq("fn") => {
                    let loc = token.loc;
                    let token = self.expect_kind(Ident)?;
                    let name = token.value;
                    self.expect_kind(ParenOpen)?;
                    let (args, ret_type) = self.parse_fn_type()?;
                    let body = self.parse_expr_list(|tk| tk.val_eq("end"))?;
                    let expr = Function {
                        loc,
                        name,
                        args,
                        ret_type,
                        body,
                    };
                    self.module.funcs.push(expr);
                }
                _ => {
                    chs_error!(
                        "{} Invalid Expression on top level {}('{}')",
                        token.loc,
                        token.kind,
                        token.value
                    )
                }
            }
        }
        Ok(self.module)
    }

    fn parse_expression(&mut self, precedence: Precedence) -> Result<Expression, CHSError> {
        use chs_lexer::TokenKind::*;
        let token = self.next();
        let mut left: Expression = match token.kind {
            Ident if self.peek().kind == Colon => {
                self.next();
                let ttype = self.parse_type()?;
                if ttype.is_some() {
                    self.expect_kind(Assign)?;
                }
                let value = self.parse_expression(Precedence::Lowest)?;
                let name = token.value;
                Expression::VarDecl(Box::new(VarDecl {
                    loc: token.loc,
                    name,
                    ttype,
                    value,
                }))
            }
            Keyword if token.val_eq("set") => {
                let loc = token.loc;
                let assined = self.parse_expression(Precedence::Lowest)?;
                self.expect_kind(Assign)?;
                let value = self.parse_expression(Precedence::Lowest)?;
                Expression::Assign(Box::new(nodes::Assign {
                    loc,
                    assined,
                    value,
                }))
            }
            String | Ident | Interger => Expression::from_literal_token(token)?,
            Keyword if token.val_eq("true") || token.val_eq("false") => {
                Expression::from_literal_token(token)?
            }
            Ampersand | Asterisk => {
                let expr = self.parse_expression(Precedence::RefDeref)?;
                Expression::Unop(
                    Unop {
                        op: Operator::from_token(&token, true)?,
                        loc: token.loc,
                        left: expr,
                    }
                    .into(),
                )
            }
            Bang | Minus => {
                let expr = self.parse_expression(Precedence::Prefix)?;
                Expression::Unop(
                    Unop {
                        op: Operator::from_token(&token, true)?,
                        loc: token.loc,
                        left: expr,
                    }
                    .into(),
                )
            }
            ParenOpen if self.peek().kind == ParenClose => {
                Expression::ConstExpression(ConstExpression::Void)
            }
            ParenOpen => {
                let expr = self.parse_expression(Precedence::Lowest)?;
                self.expect_kind(ParenClose)?;
                Expression::Group(expr.into())
            }
            CurlyOpen => {
                self.parse_init_list()?
            }
            _ => chs_error!(
                "{} Unexpected token {}('{}')",
                token.loc,
                token.kind,
                token.value
            ),
        };
        loop {
            let ptoken = self.peek();
            match ptoken.kind {
                ParenOpen => {
                    let ptoken = self.next();
                    let args = self.parse_expr_list(|tk| tk.kind == ParenClose)?;
                    let call = Expression::Call(
                        Call {
                            loc: ptoken.loc,
                            caller: left,
                            args,
                        }
                        .into(),
                    );
                    left = call;
                    return Ok(left);
                }
                Plus | Asterisk | Slash | Minus | Eq | NotEq => {
                    let operator = Operator::from_token(&ptoken, false)?;
                    if precedence < operator.precedence() {
                        let loc = self.next().loc;
                        let infix = self.parse_infix_expression(loc, operator, left)?;
                        left = infix
                    } else {
                        return Ok(left);
                    }
                }
                _ => return Ok(left),
            }
        }
    }

    fn parse_infix_expression(
        &mut self,
        loc: Loc,
        op: Operator,
        left: Expression,
    ) -> Result<Expression, CHSError> {
        let right = self.parse_expression(op.precedence())?;
        Ok(Expression::Binop(
            Binop {
                loc,
                op,
                right,
                left,
            }
            .into(),
        ))
    }

    fn parse_init_list(&mut self) -> Result<Expression, CHSError>
    {
        use chs_lexer::TokenKind::*;
        let mut args = vec![];
        loop {
            let ptoken = self.peek();
            match ptoken.kind {
                CurlyClose => {
                    self.next();
                    return Ok(Expression::ExpressionList(args))
                },
                Ident => {
                    let token = self.next();
                    let ntoken = self.next();
                    if ntoken.kind == Assign {
                        args.push(Expression::Assign(nodes::Assign {
                            loc: token.loc,
                            assined: Expression::ConstExpression(ConstExpression::Symbol(token.value)),
                            value: self.parse_expression(Precedence::Lowest)?,
                        }.into()));
                        continue;
                    } else {
                        self.peeked = Some(ntoken);
                    }
                }
                Comma => {
                    self.next();
                    continue;
                }
                _ => {}
            }
            let value = self.parse_expression(Precedence::Lowest)?;
            args.push(value);
        }
    }

    fn parse_expr_list<F>(&mut self, pred: F) -> Result<Vec<Expression>, CHSError>
    where
        F: Fn(&Token) -> bool,
    {
        use chs_lexer::TokenKind::*;
        let mut args = vec![];
        loop {
            let ptoken = self.peek();
            match ptoken.kind {
                _ if pred(ptoken) => {
                    self.next();
                    return Ok(args);
                }
                Comma => {
                    self.next();
                    continue;
                }
                _ => {
                    let value = self.parse_expression(Precedence::Lowest)?;
                    args.push(value);
                }
            }
        }
    }

    fn parse_fn_type(&mut self) -> Result<(Vec<(String, CHSType)>, CHSType), CHSError> {
        use chs_lexer::TokenKind::*;
        let mut list = vec![];
        let mut ret_type = CHSType::void();
        loop {
            let ptoken = self.peek();
            match ptoken.kind {
                ParenClose => {
                    self.next();
                    let ptoken = self.peek();
                    if ptoken.kind == Arrow {
                        self.next();
                        if let Some(value) = self.parse_type()? {
                            ret_type = value;
                        }
                    }
                    return Ok((list, ret_type));
                }
                Comma => {
                    self.next();
                    continue;
                }
                Ident => {
                    let token = self.next();
                    self.expect_kind(Colon)?;
                    if let Some(value) = self.parse_type()? {
                        list.push((token.value, value));
                    } else {
                        return Ok((list, ret_type));
                    }
                }
                _ => chs_error!(""),
            }
        }
    }

    fn parse_type(&mut self) -> Result<Option<CHSType>, CHSError> {
        use chs_lexer::TokenKind::*;
        let ttoken = self.next();
        let ttype = match ttoken.kind {
            Ident if ttoken.val_eq("int") => Some(CHSType::int()),
            Ident if ttoken.val_eq("bool") => Some(CHSType::bool()),
            Ident if ttoken.val_eq("char") => Some(CHSType::char()),
            Ident if ttoken.val_eq("void") => Some(CHSType::void()),
            Ident => Some(CHSType::custom(ttoken.value)),
            Asterisk => {
                if let Some(ttp) = self.parse_type()? {
                    Some(CHSType::Pointer(ttp.into()))
                } else {
                    chs_error!("Expect type")
                }
            }
            CurlyOpen => {
                let mut map = BTreeMap::new();
                loop {
                    let field = self.expect_kind(Ident)?;
                    self.expect_kind(Colon)?;
                    if let Some(field_type) = self.parse_type()? {
                        map.insert(field.value, field_type);
                    } else {
                        chs_error!("Expect type");
                    }
                    if self.next().kind == CurlyClose { break; }
                }
                Some(CHSType::Record(map))
            }
            ParenOpen => {
                let mut tuple = Vec::new();
                loop {
                    if let Some(tty) = self.parse_type()? {
                        tuple.push(tty);
                    } else {
                        chs_error!("Expect type");
                    }
                    if self.next().kind == ParenClose { break; }
                }
                Some(CHSType::Tuple(tuple))
            }
            Assign => None,
            _ => chs_error!("Type not implemnted {}", ttoken),
        };
        Ok(ttype)
    }
}
