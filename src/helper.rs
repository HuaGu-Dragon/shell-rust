use std::borrow::Cow;
use std::collections::HashMap;
use std::process::Command;

use rustyline::Changeset;
use rustyline::CompletionType;

use rustyline::Helper;
use rustyline::completion::Candidate;
use rustyline::completion::Completer;
use rustyline::completion::FilenameCompleter;
use rustyline::completion::Pair;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::line_buffer::LineBuffer;
use rustyline::validate::Validator;

use crate::PROGRAMS;

pub struct ShellHelper {
    pub completer: FilenameCompleter,
    pub custom_completion: HashMap<String, String>,
}

impl ShellHelper {
    fn run_completer_script(&self, cmd: &str, cur: &str, prev: &str) -> Vec<String> {
        let Some(path) = self.custom_completion.get(cmd) else {
            return vec![];
        };
        let Ok(output) = Command::new(path).arg(cmd).arg(cur).arg(prev).output() else {
            return vec![];
        };
        let result = String::from_utf8_lossy(&output.stdout);

        result.lines().map(|s| s.trim().to_string()).collect()
    }

    fn complete_command(&self, partial: &str) -> Vec<Pair> {
        let mut candidates = Vec::new();

        for cmd in ["echo", "exit", "history"].iter() {
            if cmd.starts_with(partial) {
                candidates.push(Pair {
                    display: cmd.to_string(),
                    replacement: cmd.to_string(),
                });
            }
        }

        for cmd in PROGRAMS.as_slice() {
            if cmd.starts_with(partial) {
                candidates.push(Pair {
                    display: cmd.clone(),
                    replacement: cmd.clone(),
                });
            }
        }

        candidates
    }

    fn complete_filenames(
        &self,
        line: &str,
        pos: usize,
        ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let (start, mut complete) = self.completer.complete(line, pos, ctx)?;

        for pair in complete.iter_mut() {
            if pair.replacement.ends_with('/') {
                pair.display.push('/');
            } else if !pair.replacement.ends_with(' ') {
                pair.replacement.push(' ');
            }
        }

        Ok((start, complete))
    }
}

impl Hinter for ShellHelper {
    type Hint = String;
}

impl Validator for ShellHelper {}

impl Highlighter for ShellHelper {
    fn highlight_candidate<'c>(
        &self,
        candidate: &'c str,
        completion: CompletionType,
    ) -> Cow<'c, str> {
        let _ = completion;
        Cow::Borrowed(candidate)
    }
}

impl Helper for ShellHelper {}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let partial = &line[..pos];
        let trimmed = partial.trim();

        let last_word_start = if let Some(space_pos) = partial.rfind(char::is_whitespace) {
            space_pos + 1
        } else {
            0
        };

        let last_word = &partial[last_word_start..];

        if partial.ends_with(char::is_whitespace) {
            let mut commands = trimmed.split_whitespace();
            if let Some(cmd) = commands.next() {
                let prev = commands.next_back().unwrap_or("");
                let completions = self.run_completer_script(cmd, "", prev);
                if completions.is_empty() {
                    return self.complete_filenames(line, pos, ctx);
                }

                return Ok((
                    last_word_start,
                    completions
                        .into_iter()
                        .map(|c| Pair {
                            display: c.clone(),
                            replacement: format!("{c} "),
                        })
                        .collect(),
                ));
            }

            self.complete_filenames(line, pos, ctx)
        } else {
            let mut commands = trimmed.split_whitespace();
            if let (Some(cmd), Some(cur)) = (commands.next(), commands.next_back()) {
                let prev = commands.next_back().unwrap_or("");
                let completions = self.run_completer_script(cmd, cur, prev);
                if completions.is_empty() {
                    return self.complete_filenames(line, pos, ctx);
                }

                return Ok((
                    last_word_start,
                    completions
                        .into_iter()
                        .map(|c| Pair {
                            display: c.clone(),
                            replacement: format!("{c} "),
                        })
                        .collect(),
                ));
            }

            let mut candidates = self.complete_command(last_word);

            if candidates.is_empty() {
                self.complete_filenames(line, pos, ctx)
            } else {
                candidates.sort_unstable_by(|c1, c2| c1.display().cmp(c2.display()));
                Ok((last_word_start, candidates))
            }
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
