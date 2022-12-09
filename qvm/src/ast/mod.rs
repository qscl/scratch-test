use colored::*;
use sqlparser::ast as sqlast;

pub use sqlparser::tokenizer::Location;

pub trait Pretty {
    fn pretty(&self) -> String;
}

#[derive(Clone, Debug)]
pub enum SourceLocation {
    Unknown,
    File(String),
    Single(String, Location),
    Range(String, Location, Location),
}

impl SourceLocation {
    pub fn annotate(&self, code: &str) -> Option<String> {
        let lines = code.lines().collect::<Vec<_>>();
        let (start_col, end_col, lines) = match self {
            SourceLocation::Unknown | SourceLocation::File(_) => return None,
            SourceLocation::Single(_, l) => (
                l.column,
                l.column,
                &lines[(l.line as usize) - 1..l.line as usize],
            ),
            SourceLocation::Range(_, s, e) => (
                s.column,
                e.column,
                &lines[s.line as usize - 1..e.line as usize],
            ),
        };

        let num_lines = lines.len();
        if num_lines == 0 {
            return None;
        }

        let lines = lines
            .iter()
            .enumerate()
            .flat_map(|(i, l)| {
                let start = if i == 0 { start_col } else { 0 };
                let end = if i == num_lines - 1 {
                    end_col
                } else {
                    (l.len() - 1) as u64
                };
                let annotation = (0..start + 1).map(|_| " ").collect::<String>()
                    + (start..end + 1).map(|_| "^").collect::<String>().as_str();
                vec![
                    "  ".to_string() + l.to_string().as_str(),
                    annotation.bright_red().to_string(),
                ]
            })
            .collect::<Vec<_>>();

        Some(lines.join("\n"))
    }

    pub fn file(&self) -> Option<String> {
        match self {
            SourceLocation::Unknown => None,
            SourceLocation::File(file) => Some(file.clone()),
            SourceLocation::Single(file, _) => Some(file.clone()),
            SourceLocation::Range(file, _, _) => Some(file.clone()),
        }
    }
}

impl Pretty for SourceLocation {
    fn pretty(&self) -> String {
        match self {
            SourceLocation::Unknown => "<qvm>".white().bold().to_string(),
            SourceLocation::File(f) => f.clone().white().bold().to_string(),
            SourceLocation::Single(f, l) => format!("{}:{}:{}", f, l.line, l.column)
                .white()
                .bold()
                .to_string(),
            SourceLocation::Range(f, s, e) => {
                format!("{}:{}:{}-{}:{}", f, s.line, s.column, e.line, e.column)
                    .white()
                    .bold()
                    .to_string()
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Ident {
    pub loc: SourceLocation,
    pub value: String,
}

impl Ident {
    pub fn with_location(loc: SourceLocation, value: String) -> Ident {
        Ident { loc, value }
    }

    pub fn without_location(value: String) -> Ident {
        Ident {
            loc: SourceLocation::Unknown,
            value,
        }
    }

    pub fn to_sqlident(&self) -> sqlast::Ident {
        sqlast::Ident {
            value: self.value.clone(),
            quote_style: Some('\"'),
        }
    }
}

pub type Path = Vec<Ident>;

impl Pretty for Path {
    fn pretty(&self) -> String {
        sqlast::ObjectName(self.iter().map(|i| i.to_sqlident()).collect())
            .to_string()
            .white()
            .bold()
            .to_string()
    }
}

pub trait ToStrings {
    fn to_strings(&self) -> Vec<String>;
}

impl ToStrings for Path {
    fn to_strings(&self) -> Vec<String> {
        self.iter().map(|i| i.value.clone()).collect()
    }
}

pub trait ToPath {
    fn to_path(&self, loc: &SourceLocation) -> Path;
}

impl ToPath for &Vec<sqlast::Ident> {
    fn to_path(&self, loc: &SourceLocation) -> Path {
        self.iter()
            .map(|p| {
                // XXX: This is wrong, since we're putting the same location onto each element of
                // the path.  Given that we don't actually know whether the location passed in here
                // is correct anyway, this is the safest way to ensure that the overall location is
                // preserved.
                //
                Ident::with_location(
                    loc.clone(),
                    match p.quote_style {
                        Some(_) => p.value.clone(), // Preserve the case if the string is quoted
                        None => p.value.to_lowercase(),
                    },
                )
            })
            .collect()
    }
}

impl ToPath for &sqlast::ObjectName {
    fn to_path(&self, loc: &SourceLocation) -> Path {
        (&self.0).to_path(loc)
    }
}

#[derive(Clone, Debug)]
pub struct NameAndType {
    pub name: Ident,
    pub def: Type,
}

#[derive(Clone, Debug)]
pub struct NameAndExpr {
    pub name: Ident,
    pub expr: Option<Expr>,
}

#[derive(Clone, Debug)]
pub enum StructEntry {
    NameAndType(NameAndType),
    Include(Path),
}

#[derive(Clone, Debug)]
pub enum TypeBody {
    Reference(Path),
    Struct(Vec<StructEntry>),
    List(Box<Type>),
    Exclude {
        inner: Box<Type>,
        excluded: Vec<Ident>,
    },
}

#[derive(Clone, Debug)]
pub struct Type {
    pub body: TypeBody,
    pub start: Location,
    pub end: Location,
}

#[derive(Clone, Debug)]
pub struct FnArg {
    pub name: Ident,
    pub type_: Type,
}

#[derive(Clone, Debug)]
pub enum ExprBody {
    SQLQuery(sqlast::Query),
    SQLExpr(sqlast::Expr),
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub body: ExprBody,
    pub start: Location,
    pub end: Location,
}

#[derive(Clone, Debug)]
pub enum ImportList {
    None,
    Star,
    Items(Vec<Path>),
}

#[derive(Clone, Debug)]
pub enum FnBody {
    Expr(Expr),
    Native, // A NativeFn (e.g. load)
    SQL,    // A function we expect to exist in the SQL runtime
}

#[derive(Clone, Debug)]
pub enum StmtBody {
    Noop,
    Expr(Expr),
    Import {
        path: Path,
        list: ImportList,
        args: Option<Vec<NameAndExpr>>,
    },
    TypeDef(NameAndType),
    FnDef {
        name: Ident,
        generics: Vec<Ident>,
        args: Vec<FnArg>,
        ret: Option<Type>,
        body: FnBody,
    },
    Let {
        name: Ident,
        type_: Option<Type>,
        body: Expr,
    },
    Extern {
        name: Ident,
        type_: Type,
    },
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub export: bool,
    pub body: StmtBody,
    pub start: Location,
    pub end: Location,
}

#[derive(Clone, Debug)]
pub struct Schema {
    pub stmts: Vec<Stmt>,
}
