#[allow(unused_imports)]
use std::io::{self, Write};

use anyhow::Context;

fn main() -> anyhow::Result<()> {
    // TODO: Uncomment the code below to pass the first stage
    print!("$ ");
    io::stdout().flush().unwrap();

    let mut buf = String::new();
    let input = io::stdin()
        .read_line(&mut buf)
        .context("read user input into buf")?;

    let com = buf.trim();
    println!("{com}: command not found");

    Ok(())
}
