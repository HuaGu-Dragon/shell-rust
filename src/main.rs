#[allow(unused_imports)]
use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::Context;

enum Command {
    Exit,
    Echo,
    Type,
    Path(PathBuf),
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
            Some(Command::Echo) => println!("{args}"),
            Some(Command::Path(_)) => unimplemented!(),
            Some(Command::Exit) => break,
            Some(Command::Type) => {
                let command = command_type(args);
                match command {
                    Some(Command::Path(ref path)) => println!("{args} is {}", path.display()),
                    Some(_) => println!("{args} is a shell builtin"),
                    None => println!("{args}: not found"),
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
        _ => std::env::var_os("PATH").and_then(|paths| {
            for path in std::env::split_paths(&paths) {
                if path.is_dir() {
                    for entry in path.read_dir().ok()?.flatten() {
                        if com == entry.file_name() {
                            return Some(Command::Path(entry.path()));
                        }
                    }
                }
                if path.is_file() && path.file_name().unwrap().to_str().unwrap() == com {
                    return Some(Command::Path(path));
                }
            }
            None
        }),
    }
}
