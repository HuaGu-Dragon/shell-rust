use std::borrow::Cow;
use std::ffi::OsStr;
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

        let input = buf.trim();
        let (com, args) = match input.split_once(' ') {
            Some((com, args)) => (com.trim(), args.trim()),
            None => (input, ""),
        };

        let command = command_type(com);

        match command {
            Some(Command::Echo) => {
                for arg in Parser::new(args) {
                    print!("{} ", arg.to_string_lossy());
                }
                println!();
            }
            Some(Command::Cd) => {
                let mut path = PathBuf::from(args);
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
fn run_command(path: &Path, _: &str, args: &str) -> anyhow::Result<()> {
    let mut child = std::process::Command::new(path)
        .args(Parser::new(args))
        .spawn()
        .context("spawn child process")?;

    child.wait().context("wait for child process")?;
    Ok(())
}

#[cfg(unix)]
fn run_command(path: &Path, com: &str, args: &str) -> anyhow::Result<()> {
    let mut child = std::process::Command::new(path)
        .arg0(com)
        .args(Parser::new(args))
        .spawn()
        .context("spawn child process")?;

    child.wait().context("wait for child process")?;
    Ok(())
}

struct Parser<'de> {
    args: &'de str,
    start: usize,
}

impl<'de> Parser<'de> {
    fn new(args: &'de str) -> Self {
        Parser { args, start: 0 }
    }
}

impl<'de> Iterator for Parser<'de> {
    type Item = Cow<'de, OsStr>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let args = &self.args[self.start..];
            match args.bytes().next()? {
                b'"' => {
                    let offset = '"'.len_utf8();
                    let end = args[offset..].find('"')? + offset;
                    let arg: Cow<'_, OsStr> = Cow::Borrowed(args[offset..end].as_ref());

                    self.args = &args[end + 1..];
                    self.start = 0;

                    if arg.is_empty() {
                        continue;
                    }
                    break Some(arg);
                }
                b'\'' => {
                    let offset = '\''.len_utf8();
                    let mut end = args[offset..].find('\'')? + offset;
                    let mut arg: Cow<'_, OsStr> = Cow::Borrowed(args[offset..end].as_ref());

                    while let Some(b'\'') = args[end + 1..].bytes().next() {
                        let start = end + 1 + offset;
                        end += args[end + offset + 1..].find('\'')? + offset + 1;

                        arg.to_mut().push(&args[start..end]);
                    }

                    self.args = &args[end + 1..];
                    self.start = 0;

                    break Some(arg);
                }
                b' ' => self.start += 1,
                _ => {
                    let end = args.find([' ', '\'', '"']).unwrap_or(args.len());
                    let mut arg: Cow<'_, OsStr> = Cow::Borrowed(args[..end].as_ref());

                    self.args = &args[end..];
                    self.start = 0;

                    if end != args.len()
                        && let Some(b'\'') = args[end..].bytes().next()
                    {
                        arg.to_mut().push(self.next()?);
                        if let Some(next) = self.next() {
                            arg.to_mut().push(next);
                        }
                    }

                    break Some(arg);
                }
            }
        }
    }
}

#[test]
fn test_parser() {
    let mut parser = Parser::new("arg1 'arg2' arg3 'ar''g''4'");
    assert_eq!(parser.next().as_deref(), Some(OsStr::new("arg1")));
    assert_eq!(parser.next().as_deref(), Some(OsStr::new("arg2")));
    assert_eq!(parser.next().as_deref(), Some(OsStr::new("arg3")));
    assert_eq!(parser.next().as_deref(), Some(OsStr::new("arg4")));
    assert_eq!(parser.next(), None);
}
