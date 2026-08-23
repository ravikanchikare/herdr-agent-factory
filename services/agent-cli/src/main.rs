//! The verbs an Orchestrator uses to drive its own Factory Run.
//!
//! An Orchestrator runs inside a Herdr pane, so it already drives agents with
//! the `herdr` CLI: split a pane, start an agent, prompt it, wait on it, read
//! it. What it cannot do for itself is apply an Environment — the resolved
//! provider gateway, model, secrets, skills and permissions never travel with a
//! pane it splits on its own, and carrying them by hand would pull credentials
//! through a context window.
//!
//! So this covers exactly the moves where an Environment boundary or a durable
//! transition is at stake, and leaves the loop itself to the agent.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use agent_control::{
    ControlCommand, ControlRequest, ControlResponse, ENDPOINT_ENV, FinishVerdict, TOKEN_ENV,
};

/// Long enough to cover starting an agent, which waits for Herdr to see it.
const TIMEOUT: Duration = Duration::from_secs(90);

const USAGE: &str = "\
agent-factory — drive the Factory Run you are orchestrating

USAGE:
  agent-factory status
  agent-factory start coding --brief <text>
  agent-factory start evaluation [--brief <text>]
  agent-factory escalate --question <text>
  agent-factory finish --verdict <pass|needs-review> --summary <text>

Every command acts on the Run this Orchestrator was started for; there is no
Run to name. Output is JSON on stdout. A refusal exits 2 with the reason on
stderr, and is safe to correct and retry.

Run the loop unattended. Use `escalate` only when the decision is genuinely not
yours to make: it keeps the Run and your pane, the answer is typed to you, and
your next command clears the question.

`start coding` a second time begins the next iteration.
After starting an agent, prompt it by name with `herdr agent prompt <name>`.";

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(Failure::Usage(message)) => {
            eprintln!("{message}\n\n{USAGE}");
            ExitCode::from(64)
        }
        Err(Failure::Refused { code, message }) => {
            eprintln!("{code}: {message}");
            ExitCode::from(2)
        }
        Err(Failure::Unavailable(message)) => {
            eprintln!("{message}");
            ExitCode::from(69)
        }
    }
}

#[derive(Debug)]
enum Failure {
    /// The command line is wrong. Nothing was attempted.
    Usage(String),
    /// The runtime understood and declined. The message says how to correct it.
    Refused { code: String, message: String },
    /// Agent Factory could not be reached at all.
    Unavailable(String),
}

fn run() -> Result<String, Failure> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments
        .iter()
        .any(|value| value == "-h" || value == "--help")
    {
        return Ok(USAGE.to_owned());
    }
    let command = parse(&arguments)?;

    let endpoint = std::env::var_os(ENDPOINT_ENV).ok_or_else(|| {
        Failure::Unavailable(format!(
            "{ENDPOINT_ENV} is not set. This command only works inside an Orchestrator \
             started by Agent Factory."
        ))
    })?;
    let token = std::env::var(TOKEN_ENV).map_err(|_| {
        Failure::Unavailable(format!(
            "{TOKEN_ENV} is not set, so this process is not authorized to drive a Run."
        ))
    })?;

    let response = agent_control::call(
        &PathBuf::from(endpoint),
        &ControlRequest { token, command },
        TIMEOUT,
    )
    .map_err(|error| Failure::Unavailable(error.to_string()))?;

    match response {
        ControlResponse::Ok(view) => serde_json::to_string_pretty(&view)
            .map_err(|error| Failure::Unavailable(error.to_string())),
        ControlResponse::Error { code, message } => Err(Failure::Refused { code, message }),
    }
}

fn parse(arguments: &[String]) -> Result<ControlCommand, Failure> {
    let words: Vec<&str> = arguments.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] => Err(Failure::Usage("no command given".into())),
        ["status", rest @ ..] => {
            reject_extra(rest)?;
            Ok(ControlCommand::Status)
        }
        ["start", "coding", rest @ ..] => Ok(ControlCommand::StartCoding {
            brief: required(rest, "--brief", "start coding")?,
        }),
        ["start", "evaluation", rest @ ..] => Ok(ControlCommand::StartEvaluation {
            brief: optional(rest, "--brief")?,
        }),
        ["start", other, ..] => Err(Failure::Usage(format!(
            "`start {other}` is not a phase; use `coding` or `evaluation`"
        ))),
        ["escalate", rest @ ..] => Ok(ControlCommand::Escalate {
            question: required(rest, "--question", "escalate")?,
        }),
        ["finish", rest @ ..] => {
            let verdict = match required(rest, "--verdict", "finish")?.as_str() {
                "pass" => FinishVerdict::Pass,
                "needs-review" | "needs_review" => FinishVerdict::NeedsReview,
                other => {
                    return Err(Failure::Usage(format!(
                        "`{other}` is not a verdict; use `pass` or `needs-review`"
                    )));
                }
            };
            Ok(ControlCommand::Finish {
                verdict,
                summary: required(rest, "--summary", "finish")?,
            })
        }
        [other, ..] => Err(Failure::Usage(format!("`{other}` is not a command"))),
    }
}

fn optional(arguments: &[&str], flag: &str) -> Result<Option<String>, Failure> {
    let Some(index) = arguments.iter().position(|value| *value == flag) else {
        return Ok(None);
    };
    arguments
        .get(index + 1)
        .map(|value| Some((*value).to_owned()))
        .ok_or_else(|| Failure::Usage(format!("{flag} needs a value")))
}

fn required(arguments: &[&str], flag: &str, command: &str) -> Result<String, Failure> {
    optional(arguments, flag)?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Failure::Usage(format!("{command} needs {flag}")))
}

fn reject_extra(arguments: &[&str]) -> Result<(), Failure> {
    match arguments.first() {
        Some(unexpected) => Err(Failure::Usage(format!(
            "unexpected argument `{unexpected}`"
        ))),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_words(words: &[&str]) -> Result<ControlCommand, Failure> {
        let owned: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
        parse(&owned)
    }

    #[test]
    fn start_coding_carries_its_brief() {
        assert_eq!(
            parse_words(&["start", "coding", "--brief", "Do the thing"]).unwrap(),
            ControlCommand::StartCoding {
                brief: "Do the thing".into()
            }
        );
    }

    #[test]
    fn evaluation_may_run_without_a_brief() {
        assert_eq!(
            parse_words(&["start", "evaluation"]).unwrap(),
            ControlCommand::StartEvaluation { brief: None }
        );
    }

    #[test]
    fn finish_accepts_both_spellings_of_needs_review() {
        for spelling in ["needs-review", "needs_review"] {
            let parsed =
                parse_words(&["finish", "--verdict", spelling, "--summary", "Not sure"]).unwrap();
            assert_eq!(
                parsed,
                ControlCommand::Finish {
                    verdict: FinishVerdict::NeedsReview,
                    summary: "Not sure".into()
                }
            );
        }
    }

    #[test]
    fn escalation_carries_the_question_it_needs_answered() {
        assert_eq!(
            parse_words(&["escalate", "--question", "Which database?"]).unwrap(),
            ControlCommand::Escalate {
                question: "Which database?".into()
            }
        );
        assert!(matches!(parse_words(&["escalate"]), Err(Failure::Usage(_))));
    }

    #[test]
    fn a_missing_required_flag_is_a_usage_error_not_a_call() {
        assert!(matches!(
            parse_words(&["start", "coding"]),
            Err(Failure::Usage(_))
        ));
        assert!(matches!(
            parse_words(&["finish", "--verdict", "pass"]),
            Err(Failure::Usage(_))
        ));
        assert!(matches!(
            parse_words(&["finish", "--verdict", "maybe", "--summary", "x"]),
            Err(Failure::Usage(_))
        ));
    }

    #[test]
    fn an_empty_brief_is_refused_before_it_reaches_the_runtime() {
        assert!(matches!(
            parse_words(&["start", "coding", "--brief", "   "]),
            Err(Failure::Usage(_))
        ));
    }
}
