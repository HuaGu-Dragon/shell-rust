use std::borrow::Cow;
use std::fs::File;
use std::io::Write;

use std::path::Path;
use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::Context;
use rustyline::CompletionType;
use rustyline::Config;
use rustyline::completion::Candidate;
use rustyline::completion::Completer;
use rustyline::completion::FilenameCompleter;
use rustyline::completion::Pair;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Editor, Helper};
use shlex::Shlex;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::process::CommandExt;

static PROGRAMS: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut programs = Vec::new();
    std::env::var_os("PATH").iter().for_each(|paths| {
        for path in std::env::split_paths(&paths) {
            if path.is_dir()
                && let Ok(dir) = path.read_dir()
            {
                for entry in dir.flatten() {
                    if let Some(program) = entry.path().file_stem()
                        && is_executable(&entry.path())
                    {
                        programs.push(program.to_string_lossy().into());
                    }
                }
            }
            if let Some(program) = path.as_path().file_stem()
                && is_executable(&path)
            {
                programs.push(program.to_string_lossy().into());
            }
        }
    });
    programs
});

enum Command {
    Exit,
    Echo,
    Pwd,
    Cd,
    Type,
    Program(PathBuf),
}

struct ShellHelper {
    completer: FilenameCompleter,
}

impl Hinter for ShellHelper {
    type Hint = String;
}

impl Validator for ShellHelper {}

impl Highlighter for ShellHelper {
    fn highlight_candidate<'c>(
        &self,
        candidate: &'c str, // FIXME should be Completer::Candidate
        completion: CompletionType,
    ) -> Cow<'c, str> {
        let _ = completion;
        Cow::Borrowed(candidate)
    }
}

impl Helper for ShellHelper {}

impl Completer for ShellHelper {
    type Candidate = Pair;
    // TODO: let the implementers choose/find word boundaries ??? => Lexer

    /// Takes the currently edited `line` with the cursor `pos`ition and
    /// returns the start position and the completion candidates for the
    /// partial word to be completed.
    ///
    /// `("ls /usr/loc", 11)` => `Ok((3, vec!["/usr/local/"]))`
    fn complete(
        &self, // FIXME should be `&mut self`
        line: &str,
        pos: usize,
        ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let mut commands = vec![String::from("echo"), String::from("exit")];
        commands.extend_from_slice(PROGRAMS.as_slice());

        let mut com = commands
            .into_iter()
            .filter(|c| c.starts_with(&line[..pos]))
            .map(|c| Pair {
                display: c.clone(),
                replacement: c,
            })
            .collect::<Vec<_>>();
        if com.is_empty() {
            self.completer.complete(line, pos, ctx)
        } else if com.len() == 1 {
            com[0].display.push(' ');
            com[1].replacement.push(' ');
            Ok((0, com))
        } else {
            com.sort_unstable_by(|c1, c2| c1.display().cmp(c2.display()));
            Ok((0, com))
        }
    }
}

fn main() -> anyhow::Result<()> {
    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();
    let mut rl = Editor::with_config(config).context("create rustyline instance")?;

    let h = ShellHelper {
        completer: FilenameCompleter::new(),
    };
    rl.set_helper(Some(h));

    loop {
        let readline = rl.readline("$ ").context("read user input")?;

        let mut input = Shlex::new(readline.trim());
        let com = input.next().context("parsing command")?;
        let mut args = input;

        let command = command_type(&com);

        match command {
            Some(Command::Echo) => {
                let mut args = Parser::new(args);
                let arg = args.collect::<Vec<_>>().join(" ");
                if let Some(mut stdin) = args.stdout {
                    writeln!(&mut stdin, "{arg}").context("write to file")?;
                } else {
                    println!("{arg}");
                }
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
            Some(Command::Program(ref path)) => run_command(path, &com, Parser::new(args))?,
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
                        if entry.path().file_stem() == Some(com.as_ref())
                            && is_executable(&entry.path())
                        {
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
fn run_command(path: &Path, _: &str, mut args: Parser) -> anyhow::Result<()> {
    let mut settings = std::process::Command::new(path);
    settings.args(&mut args);

    if let Some(stdout) = args.stdout {
        settings.stdout(stdout);
    }

    if let Some(stderr) = args.stderr {
        settings.stderr(stderr);
    }

    let mut child = settings.spawn().context("spawn child process")?;

    child.wait().context("wait for child process")?;
    Ok(())
}

#[cfg(unix)]
fn run_command(path: &Path, com: &str, mut args: Parser) -> anyhow::Result<()> {
    let mut settings = std::process::Command::new(path);
    settings.arg0(com);
    settings.args(&mut args);

    if let Some(stdout) = args.stdout {
        settings.stdout(stdout);
    }

    if let Some(stderr) = args.stderr {
        settings.stderr(stderr);
    }

    let mut child = settings.spawn().context("spawn child process")?;

    child.wait().context("wait for child process")?;
    Ok(())
}

struct Parser<'de> {
    stdout: Option<File>,
    stderr: Option<File>,
    shlex: Shlex<'de>,
}

impl<'de> Parser<'de> {
    fn new(input: Shlex<'de>) -> Self {
        Self {
            stdout: None,
            stderr: None,
            shlex: input,
        }
    }
}

impl Iterator for &mut Parser<'_> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        let mut next = self.shlex.next()?;

        // TODO: Handle error
        if next == ">" || next == "1>" {
            self.stdout = Some(File::create(self.shlex.next()?).unwrap());
            next = self.shlex.next()?;
        } else if next == "2>" {
            self.stderr = Some(File::create(self.shlex.next()?).unwrap());
            next = self.shlex.next()?;
        } else if next == ">>" || next == "1>>" {
            self.stdout = Some(
                File::options()
                    .append(true)
                    .create(true)
                    .open(self.shlex.next()?)
                    .unwrap(),
            );
            next = self.shlex.next()?;
        } else if next == "2>>" {
            self.stderr = Some(
                File::options()
                    .append(true)
                    .create(true)
                    .open(self.shlex.next()?)
                    .unwrap(),
            );
            next = self.shlex.next()?;
        }

        Some(next)
    }
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
