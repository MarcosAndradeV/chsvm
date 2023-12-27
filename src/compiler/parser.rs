use std::{collections::HashMap, path::PathBuf};

use crate::{
    instructions::{Instr, Opcode},
    value::CHSValue,
};

use super::{
    lexer::{Lexer, Token, TokenKind},
    node::Program,
};

macro_rules! error {
    ($message: expr, $($field: expr),*) => {
        return Err(ParseError {
            message: format!($message, $($field),*),
        })
    };

    ($message: expr) => {
        return Err(ParseError {
            message: $message.to_string(),
        })
    }
}

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
}

pub struct Parser {
    file: PathBuf,
    lexer: Lexer,
    peeked: Option<Token>,
    pos: usize,
    labels: HashMap<String, usize>,
    label_count: usize,
}

type ResTok = Result<Token, ParseError>;

impl Parser {
    pub fn new(input: Vec<u8>, file: PathBuf) -> Self {
        let lexer = Lexer::new(input);

        Self {
            file,
            lexer,
            peeked: None,
            pos: 0,
            labels: HashMap::new(),
            label_count: 0,
        }
    }

    pub fn parse(&mut self) -> Result<Program, ParseError> {
        let mut instrs = Vec::new();

        loop {
            let token = self.next();

            if token.kind == TokenKind::Null {
                let file = self.file.clone();
                return Ok(Program { stmt: instrs, file });
            }
            instrs.append(&mut self.top_level(token)?);
        }
    }

    fn expect(&mut self, kind: TokenKind) -> ResTok {
        let token = self.next();

        if token.kind == kind {
            return Ok(token);
        }

        error!("Expect {:?} at {}", kind, self.pos)
    }

    fn expect_or(&mut self, kind: TokenKind, kind_2: TokenKind) -> ResTok {
        let token = self.next();

        if token.kind == kind || token.kind == kind_2 {
            return Ok(token);
        }

        error!("Expect {:?} or {:?} at {}", kind, kind_2, self.pos)
    }

    fn not_expect(&mut self, kind: TokenKind) -> ResTok {
        let token = self.next();

        if token.kind != kind {
            return Ok(token);
        }

        error!("Not Expect {:?} at {}", kind, self.pos)
    }

    fn next(&mut self) -> Token {
        loop {
            self.pos += 1;
            let token = self
                .peeked
                .take()
                .unwrap_or_else(|| self.lexer.next_token());

            match token.kind {
                TokenKind::Comment | TokenKind::Whitespace => {}
                _ => return token,
            }
        }
    }

    fn peek(&mut self) -> &Token {
        if self.peeked.is_none() {
            self.peeked = Some(self.next());
        }

        self.peeked.as_ref().unwrap()
    }

    fn require(&mut self) -> ResTok {
        let tok = self.next();
        if matches!(tok.kind, TokenKind::Invalid | TokenKind::Null) {
            return error!("require {:?}", tok);
        }
        Ok(tok)
    }

    fn top_level(&mut self, token: Token) -> Result<Vec<Instr>, ParseError> {
        match token.kind {
            TokenKind::Proc => self.proc(),
            _ => error!("..."),
        }
    }

    fn proc(&mut self) -> Result<Vec<Instr>, ParseError> {
        let name = self.expect(TokenKind::Identifier)?;
        self.labels.insert(name.value.clone(), self.label_count);
        self.label_count += 1;
        let _ = self.expect(TokenKind::CurlyOpen)?;
        let mut body = vec![];
        loop {
            let tok = self.require()?;
            match tok.kind {
                TokenKind::CurlyClose => break,
                TokenKind::If => {
                    self.if_block(&mut body)?;
                    continue;
                },
                
                _ => body.push(self.instr(tok)?),
            }
            
        }
        Ok(body)
    }

    fn if_block(&mut self, body: &mut Vec<Instr>) -> Result<(), ParseError> {
        self.expect(TokenKind::CurlyOpen)?;
        let offset = body.len();
        loop {
            let iftok = self.require()?;
            match iftok.kind {
                TokenKind::CurlyClose => break,
                TokenKind::If => {self.if_block(body);},
                _ => body.push(self.instr(iftok)?),
            }
        }
        body.insert(
            offset,
            Instr::new(Opcode::JmpIf,
                CHSValue::P(body.len()+1)),
            );
        Ok(())
    }

    fn instr(&mut self, tok: Token) -> Result<Instr, ParseError> {
        //let tok = self.require()?;
        let instr = match tok.kind {
            TokenKind::Int => Instr::new(
                Opcode::Pushi,
                CHSValue::I(tok.value.parse::<i64>().unwrap()),
            ),
            TokenKind::Add => Instr::new(Opcode::Add, CHSValue::none()),
            TokenKind::Minus => Instr::new(Opcode::Minus, CHSValue::none()),
            TokenKind::Mul => Instr::new(Opcode::Mul, CHSValue::none()),
            TokenKind::Div => Instr::new(Opcode::Div, CHSValue::none()),
            TokenKind::Print => Instr::new(Opcode::Print, CHSValue::none()),
            TokenKind::Pop => Instr::new(Opcode::Pop, CHSValue::none()),
            TokenKind::Hlt => Instr::new(Opcode::Halt, CHSValue::none()),
            TokenKind::Eq => Instr::new(Opcode::Eq, CHSValue::none()),
            TokenKind::Dup => Instr::new(Opcode::Dup, self.operand()?),
            _ => return error!("{:?} is not a Instr at {}", tok.value, self.pos),
        };
        Ok(instr)
    }

    fn operand(&mut self) -> Result<CHSValue, ParseError> {
        let tok = self.expect(TokenKind::Int)?;
        Ok(CHSValue::P(tok.value.parse::<usize>().unwrap()))
    }
}
