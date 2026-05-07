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
    fn get_custom_output(&self, com: &str) -> Vec<u8> {
        let Some(path) = self.custom_completion.get(com) else {
            return vec![];
        };

        let prog = Command::new(path).output();
        let Ok(output) = prog else { return vec![] };

        output.stdout
    }

    fn get_custom_completion(&self, com: &str) -> Vec<String> {
        String::from_utf8_lossy(&self.get_custom_output(com))
            .lines()
            .map(|s| s.trim().to_string())
            .collect()
    }
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

        let script = self.get_custom_completion(line[..pos].trim());
        let script_pairs = script.into_iter().map(|s| Pair {
            display: format!("{}{} ", &line[..pos], s),
            replacement: s,
        });
        com.extend(script_pairs);

        if com.is_empty() {
            let (start, mut complete) = self.completer.complete(line, pos, ctx)?;

            for pair in complete.iter_mut() {
                if pair.replacement.ends_with('/') {
                    pair.display.push('/');
                }
                if !pair.replacement.ends_with('/') && !pair.replacement.ends_with(' ') {
                    pair.replacement.push(' ');
                }
            }

            Ok((start, complete))
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
