#[allow(unused_imports)]
use std::io::{self, Write};

use anyhow::Context;

enum Command {
    Exit,
    Echo,
    Type,
    NoOp,
}

fn main() -> anyhow::Result<()> {
    // TODO: Uncomment the code below to pass the first stage

    let mut buf = String::new();
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        io::stdin()
            .read_line(&mut buf)
            .context("read user input into buf")?;

        let input = buf.trim();
        let (com, args) = match input.split_once(' ') {
            Some((com, args)) => (com.trim(), args.trim()),
            None => (input, ""),
        };

        let command = command_type(com);

        match command {
            Some(Command::Echo) => println!("{}", args),
            Some(Command::Exit) => break,
            Some(Command::Type) => {
                let command = command_type(args);
                if command.is_some() {
                    println!("{} is a shell builtin", args);
                } else {
                    println!("{}: not found", args);
                }
            }
            Some(Command::NoOp) => {}
            None => println!("{com}: command not found"),
        }

        buf.clear();
    }

    Ok(())
}

fn command_type(com: &str) -> Option<Command> {
    match com {
        "exit" => Some(Command::Exit),
        "echo" => Some(Command::Echo),
        "type" => Some(Command::Type),
        _ => None,
    }
}
