# QueryScript

QueryScript is a powerful new language that lets you perform data transformations, ETL, and BI within a single tool,
all while writing modern SQL that feels like Typescript or Rust.

With QueryScript, you get the power of SQL with the flexibility of modern programming abstractions like variables,
functions, and libraries. Under the hood, the compiler typechecks your queries and transpiles them into vanilla
ANSI SQL that runs directly against your favorite databases. QueryScript aims to do for SQL what TypeScript did
for JavaScript.

Below, we'll walk through some examples and installation instructions. You can run QueryScript locally,
making it simple and fun to develop in tools like [VSCode](https://vscode.dev/). For a more detailed
walkthrough of the language, check out the [QueryScript website](https://queryscript.com/).

## Example

Here's a quick example of QueryScript:

```sql
-- Variables (including relations) are namespaced and can be imported. The below queries will
-- work for any definition of `users` and `events` as long as they typecheck.
import users, events from schema;

-- You can define variables (including scalars and relations)
-- using `let <name> [<type>] = <expr>`. These expressions are
-- lazily evaluated (and potentially inlined), so there is no
-- cost to defining them.
let active_users = SELECT * FROM users WHERE active;

-- You can run queries inline to help with debugging
SELECT COUNT(*) FROM active_users;

-- You can define functions that operate over values. Just like
-- variables, functions can be inlined and pushed down into the
-- target database, or evaluated directly.
fn username(user_id bigint) {
    SELECT name FROM active_users WHERE id = user_id
}

-- Find the users with the most events
SELECT username(user_id), COUNT(*) FROM events GROUP BY 1 ORDER BY 2 DESC LIMIT 10;

-- Find the users who are at risk of churn
SELECT username(user_id), MAX(ts) FROM events GROUP BY 1 HAVING MAX(ts) < NOW() - INTERVAL 1 MONTH;

-- You can also define functions over relations
fn most_popular_events(events) {
    SELECT name, COUNT(*) num FROM events ORDER BY 2 DESC
}

-- And see the most popular events globally
most_popular_events(events);

-- Or per user (this will return a sorted array of events
-- for each user)
SELECT username(user_id), most_popular_events(e) FROM events e GROUP BY 1;
```

## Installation

Right now, the only way to install QueryScript is to [build it from source](#building-from-source). We'll enable a more streamlined
install in the coming weeks.

## Building from source

To build QueryScript from source, you'll need [Git](https://git-scm.com/), [Rust](https://www.rust-lang.org/tools/install), and [Clang++](https://clang.llvm.org/).

```bash
git clone https://github.com/qscl/queryscript.git
cd queryscript
git submodule update --init
cargo build --release
```

The QueryScript compiler will be available at `target/release/qs`.

## Building development environment

If you'd like to contribute or otherwise hack on QueryScript, you'll need a few more dependencies: [Python >= 3.7](https://www.python.org/downloads/), [Python venv](https://docs.python.org/3/library/venv.html), [Git LFS](https://git-lfs.com/), [Node.js](https://nodejs.org/en/download/), and [Yarn](https://classic.yarnpkg.com/en/docs/install/).

First, run the above commands (although you can skip `cargo build --release`). Then, run

```bash
make develop
```

Beyond compiling the Rust library and `qs` binary in debug mode, this command will also:

- Setup a python package named `qsutils` inside of a virtual environment with some utility scripts for downloading test data
- Install [pre-commit](https://pre-commit.com/) hooks (inside of the virtual environment).
- Install the [VSCode](https://code.visualstudio.com/) extension's dependencies and compile it.
- Download test files via `git-lfs`

Please run `make develop` if you plan on making any contributions. You can run the test suite with `make test`.
