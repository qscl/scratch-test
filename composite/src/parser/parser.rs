use crate::ast::*;
use crate::parser::error::{unexpected_token, Result};
use sqlparser::{
    // ast::{ColumnDef, ColumnOptionDef, Statement as SQLStatement, TableConstraint},
    dialect::{keywords::Keyword, GenericDialect},
    parser,
    tokenizer::{Token, Tokenizer, Word},
};

pub struct Parser<'a> {
    sqlparser: parser::Parser<'a>,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: Vec<Token>) -> Parser<'a> {
        let dialect = &GenericDialect {};
        Parser {
            sqlparser: parser::Parser::new(tokens, dialect),
        }
    }

    pub fn next_token(&mut self) -> Token {
        self.sqlparser.next_token()
    }

    pub fn peek_token(&mut self) -> Token {
        self.sqlparser.peek_token()
    }

    #[must_use]
    pub fn consume_token(&mut self, expected: &Token) -> bool {
        self.sqlparser.consume_token(expected)
    }

    pub fn expect_token(&mut self, expected: &Token) -> Result<()> {
        Ok(self.sqlparser.expect_token(expected)?)
    }

    pub fn parse_schema(&mut self) -> Result<Schema> {
        let mut stmts = Vec::new();
        while !matches!(self.peek_token(), Token::EOF) {
            stmts.push(self.parse_stmt()?);
        }

        Ok(Schema { stmts })
    }

    pub fn parse_stmt(&mut self) -> Result<Stmt> {
        let token = self.peek_token();
        let export = as_word(&token)? == "export";
        if export {
            self.next_token();
        }

        let word = as_word(&self.peek_token())?;
        let body = match word.to_lowercase().as_str() {
            "import" => {
                return unexpected_token!(self.peek_token(), "Unimplemented: import");
            }
            "fn" => {
                self.next_token();
                self.parse_fn()?
            }
            "extern" => {
                self.next_token();
                self.parse_extern()?
            }
            "let" => {
                self.next_token();
                self.parse_let()?
            }
            "type" => {
                self.next_token();
                self.parse_typedef()?
            }
            _ => {
                if export {
                    self.parse_let()?
                } else {
                    return unexpected_token!(
                        self.peek_token(),
                        "Expected: import | fn | extern | let | type"
                    );
                }
            }
        };

        // Consume (optional) semi-colons.
        while matches!(self.peek_token(), Token::SemiColon) {
            self.next_token();
        }

        Ok(Stmt { export, body })
    }

    pub fn parse_ident(&mut self) -> Result<Ident> {
        let token = self.next_token();
        match token {
            Token::Word(w) => Ok(w.value),
            Token::DoubleQuotedString(s) => Ok(s),
            _ => unexpected_token!(token, "Expected: WORD | DOUBLE_QUOTED_STRING"),
        }
    }

    pub fn parse_path(&mut self) -> Result<Path> {
        let mut path = Vec::new();
        loop {
            path.push(self.parse_ident()?);
            match self.peek_token() {
                Token::Period => {
                    self.next_token();
                }
                _ => {
                    break;
                }
            }
        }
        Ok(path)
    }

    pub fn parse_extern(&mut self) -> Result<StmtBody> {
        // Assume the leading "extern" has already been consumed
        //
        let name = self.parse_ident()?;
        let type_ = self.parse_type()?;

        Ok(StmtBody::Extern { name, type_ })
    }

    pub fn parse_idents(&mut self) -> Result<Vec<Ident>> {
        let mut ret = Vec::new();
        let mut expect_ident = true;
        loop {
            if expect_ident {
                if let Ok(ident) = self.parse_ident() {
                    ret.push(ident);
                } else {
                    break;
                }
            } else {
                match self.peek_token() {
                    Token::Comma => {}
                    _ => break,
                }

                self.next_token();
            }

            expect_ident = !expect_ident;
        }

        Ok(ret)
    }

    pub fn parse_fn(&mut self) -> Result<StmtBody> {
        // Assume the leading "fn" has already been consumed
        //
        let name = self.parse_ident()?;
        let generics = if self.consume_token(&Token::Lt) {
            let list = self.parse_idents()?;
            self.expect_token(&Token::Gt)?;

            list
        } else {
            Vec::new()
        };

        self.expect_token(&Token::LParen)?;

        let mut args = Vec::new();
        loop {
            let name = self.parse_ident()?;
            let type_ = match self.peek_token() {
                Token::Comma | Token::RParen => None,
                _ => Some(self.parse_type()?),
            };

            args.push(FnArg { name, type_ });

            match self.next_token() {
                Token::Comma => {}
                Token::RParen => break,
                _ => {
                    return unexpected_token!(self.peek_token(), "Expected: ',' | ')'");
                }
            }
        }

        let ret = if self.consume_token(&Token::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect_token(&Token::LBrace)?;

        let body = self.parse_expr()?;

        self.expect_token(&Token::RBrace)?;

        Ok(StmtBody::FnDef {
            name,
            generics,
            args,
            ret,
            body,
        })
    }

    pub fn parse_let(&mut self) -> Result<StmtBody> {
        // Assume the leading "let" or "export" keywords have already been consumed
        //
        let name = self.parse_ident()?;
        let type_ = match self.peek_token() {
            Token::Eq => None,
            _ => Some(self.parse_type()?),
        };

        let body = match self.peek_token() {
            Token::Eq => {
                self.next_token();
                self.parse_expr()?
            }
            _ => {
                return unexpected_token!(self.peek_token(), "Expected definition");
            }
        };

        Ok(StmtBody::Let { name, type_, body })
    }

    pub fn parse_typedef(&mut self) -> Result<StmtBody> {
        // Assume the leading keywords have already been consumed
        //
        let name = as_word(&self.peek_token())?.to_string();
        self.next_token();

        let def = self.parse_type()?;
        Ok(StmtBody::TypeDef(NameAndType { name, def }))
    }

    pub fn parse_type(&mut self) -> Result<Type> {
        match self.peek_token() {
            Token::Word(w) => {
                if w.value.to_lowercase() == "record" {
                    self.next_token();
                    return self.parse_struct();
                }
            }
            _ => (),
        };

        Ok(Type::Reference(self.parse_path()?))
    }

    pub fn parse_struct(&mut self) -> Result<Type> {
        self.expect_token(&Token::LBrace)?;
        let mut struct_ = Vec::new();
        let mut needs_comma = false;
        loop {
            match self.peek_token() {
                Token::RBrace => {
                    self.next_token();
                    break;
                }
                Token::Comma => {
                    if needs_comma {
                        needs_comma = false;
                    } else {
                        return unexpected_token!(self.peek_token(), "Two consecutive commas");
                    }
                    self.next_token();
                }
                Token::Period => {
                    for _ in 0..3 {
                        if !matches!(self.peek_token(), Token::Period) {
                            return unexpected_token!(self.peek_token(), "Three periods");
                        }
                        self.next_token();
                    }
                    struct_.push(StructEntry::Include(self.parse_path()?));
                    needs_comma = true;
                }
                t => {
                    if needs_comma {
                        return unexpected_token!(
                            t,
                            "Expected a comma before the next type declaration"
                        );
                    }
                    let name = self.parse_ident()?;
                    let def = self.parse_type()?;
                    struct_.push(StructEntry::NameAndType(NameAndType { name, def }));
                    needs_comma = true;
                }
            }
        }
        Ok(Type::Struct(struct_))
    }

    pub fn parse_expr(&mut self) -> Result<Expr> {
        Ok(match self.peek_token() {
            Token::Word(Word {
                value: _,
                quote_style: _,
                keyword: Keyword::SELECT,
            }) => Expr::SQLQuery(self.sqlparser.parse_query()?),
            _ => Expr::SQLExpr(self.sqlparser.parse_expr()?),
        })
    }
}

pub fn as_word(token: &Token) -> Result<String> {
    match token {
        Token::Word(w) => Ok(w.value.clone()),
        _ => unexpected_token!(token, "expecting a word"),
    }
}

pub fn tokenize(text: &str) -> Result<Vec<Token>> {
    let dialect = &GenericDialect {};
    let mut tokenizer = Tokenizer::new(dialect, text);

    Ok(tokenizer.tokenize()?)
}

pub fn parse(text: &str) -> Result<Schema> {
    let tokens = tokenize(text)?;
    eprintln!("tokens: {:#?}", tokens);
    let mut parser = Parser::new(tokens);

    parser.parse_schema()
}

// pub fn parse_sql(text: &str) {
//     let dialect = &GenericDialect {};
//     DFParser::new_with_dialect(sql, dialect).unwrap()
// }
