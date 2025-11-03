#[allow(unused_imports)]
use std::io::{self, Write};

use anyhow::Context;

enum Command {
    Exit(u8),
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

        let com = match com {
            "exit" => Command::Exit(0),
            _ => {
                println!("{com}: command not found");
                Command::NoOp
            }
        };

        match com {
            Command::Exit(code) => break,
            Command::NoOp => {}
        }

        buf.clear();
    }

    Ok(())
}
