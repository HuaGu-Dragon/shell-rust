use std::io::{self, Write};

use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::process::CommandExt;

enum Command {
    Exit,
    Echo,
    Pwd,
    Type,
    Program(PathBuf),
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
            Some(Command::Pwd) => println!(
                "{}",
                std::env::current_dir()
                    .context("get current dir")?
                    .display()
            ),
            Some(Command::Program(ref path)) => run_command(path, com, args)?,
            Some(Command::Exit) => break,
            Some(Command::Type) => {
                let command = command_type(args);
                match command {
                    Some(Command::Program(ref path)) => println!("{args} is {}", path.display()),
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
        "pwd" => Some(Command::Pwd),
        "type" => Some(Command::Type),
        _ => std::env::var_os("PATH").and_then(|paths| {
            for path in std::env::split_paths(&paths) {
                if path.is_dir() {
                    for entry in path.read_dir().ok()?.flatten() {
                        if com == entry.file_name() && is_executable(&entry.path()) {
                            return Some(Command::Program(entry.path()));
                        }
                    }
                }
                if is_executable(&path) && path.file_name()? == com {
                    return Some(Command::Program(path));
                }
            }
            None
        }),
    }
}

#[cfg(unix)]
fn is_executable(path: &PathBuf) -> bool {
    if let Ok(metadata) = path.metadata() {
        let permissions = metadata.permissions();
        permissions.mode() & 0o111 != 0
    } else {
        false
    }
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(not(unix))]
fn run_command(path: &Path, _: &str, args: &str) -> anyhow::Result<()> {
    let mut child = std::process::Command::new(path)
        .args(args.split_whitespace())
        .spawn()
        .context("spawn child process")?;

    child.wait().context("wait for child process")?;
    Ok(())
}

#[cfg(unix)]
fn run_command(path: &Path, com: &str, args: &str) -> anyhow::Result<()> {
    let mut child = std::process::Command::new(path)
        .arg0(com)
        .args(args.split_whitespace())
        .spawn()
        .context("spawn child process")?;

    child.wait().context("wait for child process")?;
    Ok(())
}
