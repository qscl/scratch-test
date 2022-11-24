use crate::ast;
use crate::compile::schema::{Decl, MType};
use crate::parser::error::ParserError;
use crate::runtime::error::RuntimeError;
use crate::types::error::TypesystemError;
use snafu::{Backtrace, Snafu};
use std::io;
pub type Result<T> = std::result::Result<T, CompileError>;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum CompileError {
    #[snafu(display("Parser error: {}", source), context(false))]
    SyntaxError {
        #[snafu(backtrace)]
        source: ParserError,
    },

    #[snafu(display("Internal error: {}", what))]
    InternalError {
        what: String,
        backtrace: Option<Backtrace>,
    },

    #[snafu(display("Typesystem error: {}", source), context(false))]
    TypesystemError {
        #[snafu(backtrace)]
        source: TypesystemError,
    },

    #[snafu(display("Parser error: {}", source), context(false))]
    RuntimeError {
        #[snafu(backtrace)]
        source: RuntimeError,
    },

    #[snafu(context(false))]
    FsError {
        source: io::Error,
        backtrace: Option<Backtrace>,
    },

    #[snafu(display("Unimplemented: {}", what))]
    Unimplemented {
        what: String,
        backtrace: Option<Backtrace>,
    },

    #[snafu(display("Duplicate entry: {:?}", path))]
    DuplicateEntry {
        path: ast::Path,
        backtrace: Option<Backtrace>,
    },

    #[snafu(display("No such entry: {:?}", path))]
    NoSuchEntry {
        path: ast::Path,
        backtrace: Option<Backtrace>,
    },

    #[snafu(display(
        "Wrong kind: declaration at {:?} is {:?} not {}",
        path,
        actual,
        expected
    ))]
    WrongKind {
        path: ast::Path,
        expected: String,
        actual: Decl,
        backtrace: Option<Backtrace>,
    },

    #[snafu(display("Type mismatch: found {:?} not {:?}", rhs, lhs))]
    WrongType {
        lhs: MType,
        rhs: MType,
        backtrace: Option<Backtrace>,
    },

    #[snafu(display("Error importing {:?}: {}", path, what))]
    ImportError {
        path: ast::Path,
        what: String,
        backtrace: Option<Backtrace>,
    },
}

impl CompileError {
    pub fn unimplemented(what: &str) -> CompileError {
        return UnimplementedSnafu { what }.build();
    }

    pub fn no_such_entry(path: ast::Path) -> CompileError {
        return NoSuchEntrySnafu { path }.build();
    }

    pub fn duplicate_entry(path: ast::Path) -> CompileError {
        return DuplicateEntrySnafu { path }.build();
    }

    pub fn wrong_kind(path: ast::Path, expected: &str, actual: &Decl) -> CompileError {
        return WrongKindSnafu {
            path,
            expected,
            actual: actual.clone(),
        }
        .build();
    }

    pub fn wrong_type(lhs: &MType, rhs: &MType) -> CompileError {
        return WrongTypeSnafu {
            lhs: lhs.clone(),
            rhs: rhs.clone(),
        }
        .build();
    }

    pub fn import_error(path: ast::Path, what: &str) -> CompileError {
        return ImportSnafu {
            path,
            what: what.to_string(),
        }
        .build();
    }

    pub fn internal(what: &str) -> CompileError {
        return InternalSnafu {
            what: what.to_string(),
        }
        .build();
    }
}

impl<Guard> From<std::sync::PoisonError<Guard>> for CompileError {
    fn from(e: std::sync::PoisonError<Guard>) -> CompileError {
        CompileError::internal(format!("{}", e).as_str())
    }
}
