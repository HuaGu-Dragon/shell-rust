use std::borrow::Cow;
use std::fs::File;
use std::io::Write;
use std::process::Stdio;

use std::path::Path;
use std::path::PathBuf;

use std::sync::LazyLock;

use anyhow::Context;
use rustyline::Changeset;
use rustyline::CompletionType;
use rustyline::Config;

use rustyline::completion::Candidate;
use rustyline::completion::Completer;
use rustyline::completion::FilenameCompleter;
use rustyline::completion::Pair;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::History;
use rustyline::line_buffer::LineBuffer;
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
    History,
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
        let mut commands = vec![
            String::from("echo"),
            String::from("exit"),
            String::from("history"),
        ];
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
        } else {
            com.sort_unstable_by(|c1, c2| c1.display().cmp(c2.display()));
            Ok((0, com))
        }
    }

    fn update(&self, line: &mut LineBuffer, start: usize, elected: &str, cl: &mut Changeset) {
        let end = line.pos();

        let mut commands = vec![String::from("echo"), String::from("exit")];
        commands.extend_from_slice(PROGRAMS.as_slice());

        let len = commands.iter().filter(|c| c.starts_with(elected)).count();

        if len == 1 || elected == "echo" || elected == "exit" {
            line.replace(start..end, &format!("{elected} "), cl);
        } else {
            line.replace(start..end, elected, cl);
        }
    }
}

fn main() -> anyhow::Result<()> {
    let config = Config::builder()
        .history_ignore_space(true)
        .auto_add_history(true)
        .completion_type(CompletionType::List)
        .build();

    let mut rl = Editor::with_config(config).context("create rustyline instance")?;

    let h = ShellHelper {
        completer: FilenameCompleter::new(),
    };
    rl.set_helper(Some(h));

    loop {
        let readline = rl.readline("$ ").context("read user input")?;

        if readline.contains('|') {
            let commands: Vec<&str> = readline.split('|').map(|s| s.trim()).collect();

            if let Err(e) = execute_pipeline(&commands) {
                eprintln!("Pipeline error: {}", e);
            }
            continue;
        }

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
            Some(Command::History) => {
                let history_info = HistoryInfo::new(args)?;
                if let Some(read) = history_info.read {
                    rl.append_history(&read).context("Read history from file")?;
                } else if let Some(num) = history_info.num {
                    let history = rl
                        .history()
                        .iter()
                        .rev()
                        .enumerate()
                        .take(num)
                        .collect::<Vec<_>>();
                    for (i, entry) in history.iter().rev() {
                        println!("  {}  {}", rl.history().len() - i, entry);
                    }
                } else {
                    rl.history()
                        .iter()
                        .enumerate()
                        .for_each(|(i, entry)| println!("    {}  {entry}", i + 1));
                }
            }
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
        "history" => Some(Command::History),
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

fn execute_pipeline(commands: &[&str]) -> anyhow::Result<()> {
    if commands.len() < 2 {
        anyhow::bail!("Pipeline must have at least 2 commands");
    }

    let mut children = Vec::new();
    let mut previous_output: Option<PipeOutput> = None;

    for (i, cmd) in commands.iter().enumerate() {
        let mut input = Shlex::new(cmd);
        let com = input.next().context("parsing command")?;
        let args = input;

        let command = command_type(&com);
        let is_last = i == commands.len() - 1;

        match command {
            Some(Command::Echo) | Some(Command::Type) | Some(Command::Pwd) => {
                if is_last {
                    execute_builtin_in_pipeline(&com, args, false)?;
                } else {
                    let output = execute_builtin_in_pipeline(&com, args, true)?;
                    previous_output = Some(output);
                }
            }
            Some(Command::Program(path)) => {
                let mut process = std::process::Command::new(&path);
                #[cfg(unix)]
                process.arg0(&com);
                process.args(args);

                match previous_output.take() {
                    Some(PipeOutput::ChildStdout(stdout)) => {
                        process.stdin(stdout);
                    }
                    Some(PipeOutput::Buffer(content)) => {
                        process.stdin(Stdio::piped());
                        let mut child = process
                            .stdout(if is_last {
                                Stdio::inherit()
                            } else {
                                Stdio::piped()
                            })
                            .spawn()
                            .context(format!("spawn process {}", i))?;

                        if let Some(mut stdin) = child.stdin.take() {
                            stdin.write_all(content.as_bytes())?;
                        }

                        if !is_last {
                            previous_output = child.stdout.take().map(PipeOutput::ChildStdout);
                        }

                        children.push(child);
                        continue;
                    }
                    None => {}
                }

                if !is_last {
                    process.stdout(Stdio::piped());
                }

                let mut child = process.spawn().context(format!("spawn process {}", i))?;

                if !is_last {
                    previous_output = child.stdout.take().map(PipeOutput::ChildStdout);
                }

                children.push(child);
            }
            Some(Command::Cd) | Some(Command::History) | Some(Command::Exit) => {
                anyhow::bail!("{} cannot be used in pipelines", com);
            }
            None => {
                anyhow::bail!("{}: command not found", com);
            }
        }
    }

    for child in children.iter_mut().rev() {
        child.wait().context("wait for process")?;
    }

    Ok(())
}

enum PipeOutput {
    ChildStdout(std::process::ChildStdout),
    Buffer(String),
}

fn execute_builtin_in_pipeline(
    com: &str,
    mut args: Shlex,
    needs_output: bool,
) -> anyhow::Result<PipeOutput> {
    let mut output = String::new();

    match com {
        "echo" => {
            let arg = args.collect::<Vec<_>>().join(" ");
            if needs_output {
                output = format!("{}\n", arg);
            } else {
                println!("{}", arg);
            }
        }
        "type" => {
            if let Some(name) = args.next() {
                let command = command_type(&name);
                let result = match command {
                    Some(Command::Program(ref path)) => format!("{} is {}", name, path.display()),
                    Some(_) => format!("{} is a shell builtin", name),
                    None => format!("{}: not found", name),
                };
                if needs_output {
                    output = format!("{}\n", result);
                } else {
                    println!("{}", result);
                }
            }
        }
        "pwd" => {
            let dir = std::env::current_dir()
                .context("get current dir")?
                .display()
                .to_string();
            if needs_output {
                output = format!("{}\n", dir);
            } else {
                println!("{}", dir);
            }
        }
        _ => anyhow::bail!("Unknown builtin: {}", com),
    }

    Ok(PipeOutput::Buffer(output))
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

struct HistoryInfo {
    read: Option<PathBuf>,
    num: Option<usize>,
}

impl HistoryInfo {
    fn new(mut shlex: Shlex<'_>) -> anyhow::Result<Self> {
        let mut read = None;
        let mut num = None;

        while let Some(next) = shlex.next() {
            match &next[..] {
                "-r" => read = Some(PathBuf::from(shlex.next().context("Load hitstory file")?)),
                _ => num = Some(next.parse().context("parsing arg into number")?),
            }
        }
        Ok(HistoryInfo { read, num })
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
