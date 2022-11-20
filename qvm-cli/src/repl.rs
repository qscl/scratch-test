use rustyline::{error::ReadlineError, Editor};
use snafu::{prelude::*, Whatever};

use qvm::compile;
use qvm::compile::schema;
use qvm::parser;
use qvm::runtime;

pub fn run() {
    let cwd = std::env::current_dir()
        .expect("current working directory")
        .display()
        .to_string();
    let repl_schema = schema::Schema::new(Some(cwd));

    let mut rl = Editor::<()>::new().expect("readline library failed");

    let qvm_dir = get_qvm_dir();
    let qvm_history = match &qvm_dir {
        Some(p) => {
            std::fs::create_dir_all(p).expect("failed to create qvm dir");
            Some(p.join("history.txt").display().to_string())
        }
        None => None,
    };

    if let Some(history_file) = &qvm_history {
        // This function returns an error when the history file does not exist,
        // which is ok.
        match rl.load_history(history_file) {
            Ok(_) => {}
            Err(_) => {}
        }
    }

    let mut curr_buffer = String::new();
    loop {
        let readline = rl.readline(if curr_buffer.len() == 0 {
            "qvm> "
        } else {
            "...> "
        });

        match readline {
            Ok(line) => {
                if curr_buffer.len() == 0 {
                    curr_buffer = line;
                } else {
                    curr_buffer.push_str(&format!("\n{}", line));
                }
                match curr_buffer.trim().to_lowercase().trim_end_matches(';') {
                    "exit" | "quit" => {
                        rl.add_history_entry(curr_buffer.as_str());
                        println!("Goodbye!");
                        break;
                    }
                    _ => {}
                };

                match run_command(repl_schema.clone(), &curr_buffer) {
                    Ok(RunCommandResult::Done) => {
                        // Reset the buffer
                        curr_buffer = String::new();
                    }
                    Ok(RunCommandResult::More) => {
                        // Allow the loop to run again (and parse more)
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        curr_buffer = String::new();
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("Interrupted...");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("Goodbye!");
                break;
            }
            Err(err) => {
                println!("I/O error: {:?}", err);
                break;
            }
        }
    }

    if let Some(history_file) = &qvm_history {
        rl.save_history(history_file)
            .expect("failed to save history");
    }
}

enum RunCommandResult {
    Done,
    More,
}

fn run_command(repl_schema: schema::SchemaRef, cmd: &str) -> Result<RunCommandResult, Whatever> {
    let tokens = parser::tokenize(&cmd).with_whatever_context(|e| format!("{}", e))?;
    let mut parser = parser::Parser::new(tokens);

    match parser.parse_schema() {
        Ok(ast) => {
            compile::compile_schema_ast(repl_schema.clone(), &ast)
                .with_whatever_context(|e| format!("{}", e))?;
            return Ok(RunCommandResult::Done);
        }
        Err(parser::ParserError::Incomplete { .. }) => return Ok(RunCommandResult::More),
        Err(_) => {
            // TODO: Currently falls back to parsing as an expression in this case.
            // We should really distinguish between whether this _is_ a schema vs.
            // an error parsing a schema.
            parser.reset();
        }
    };

    match parser.parse_expr() {
        Ok(ast) => {
            let compiled = compile::compile_expr(repl_schema.clone(), &ast)
                .with_whatever_context(|e| format!("{}", e))?;
            let expr = compiled
                .to_runtime_type()
                .with_whatever_context(|e| format!("{}", e))?;
            let value = runtime::eval(repl_schema.clone(), &expr)
                .with_whatever_context(|e| format!("{}", e))?;
            println!("{:?}", value);
        }
        Err(e) => {
            whatever!("{}", e);
        }
    };

    return Ok(RunCommandResult::Done);
}

fn get_qvm_dir() -> Option<std::path::PathBuf> {
    home::home_dir().map(|p| p.join(".qvm"))
}
