use std::io::{self, Write};

use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use shlex::Shlex;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::process::CommandExt;

enum Command {
    Exit,
    Echo,
    Pwd,
    Cd,
    Type,
    Program(PathBuf),
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

        let mut input = Shlex::new(buf.trim());
        let com = input.next().context("parsing command")?;
        let mut args = input;

        let command = command_type(&com);

        match command {
            Some(Command::Echo) => {
                for arg in args {
                    print!("{arg} ");
                }
                println!();
            }
            Some(Command::Cd) => {
                let mut path = PathBuf::from(&args.next().context("parsing path")?);
                if path.starts_with("~") {
                    let home_dir = std::env::home_dir().context("get home dir")?;
                    path = home_dir.join(path.strip_prefix("~").unwrap())
                }
                if path.is_absolute() {
                    if std::env::set_current_dir(&path).is_err() {
                        println!("cd: {}: No such file or directory", path.display())
                    }
                } else {
                    let current_dir = std::env::current_dir().context("get current dir")?;
                    let new_dir = current_dir.join(path);
                    if std::env::set_current_dir(&new_dir).is_err() {
                        println!("cd: {}: No such file or directory", new_dir.display())
                    }
                }
            }
            Some(Command::Pwd) => println!(
                "{}",
                std::env::current_dir()
                    .context("get current dir")?
                    .display()
            ),
            Some(Command::Program(ref path)) => run_command(path, &com, args)?,
            Some(Command::Exit) => break,
            Some(Command::Type) => {
                let name = &args.next().context("parsing arg")?;
                let command = command_type(name);
                match command {
                    Some(Command::Program(ref path)) => println!("{name} is {}", path.display()),
                    Some(_) => println!("{name} is a shell builtin"),
                    None => println!("{name}: not found"),
                }
            }
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
        "cd" => Some(Command::Cd),
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
fn run_command(path: &Path, _: &str, args: Shlex) -> anyhow::Result<()> {
    let mut child = std::process::Command::new(path)
        .args(args)
        .spawn()
        .context("spawn child process")?;

    child.wait().context("wait for child process")?;
    Ok(())
}

#[cfg(unix)]
fn run_command(path: &Path, com: &str, args: Shlex) -> anyhow::Result<()> {
    let mut child = std::process::Command::new(path)
        .arg0(com)
        .args(args)
        .spawn()
        .context("spawn child process")?;

    child.wait().context("wait for child process")?;
    Ok(())
}

#[test]
fn test_parser() {
    let mut parser = Shlex::new("arg1 'arg2' arg3 'ar''g''4'");
    assert_eq!(parser.next().as_deref(), Some("arg1"));
    assert_eq!(parser.next().as_deref(), Some("arg2"));
    assert_eq!(parser.next().as_deref(), Some("arg3"));
    assert_eq!(parser.next().as_deref(), Some("arg4"));
    assert_eq!(parser.next().as_deref(), None);
}
