use fleet_core::{ExecEvent, ExecRequest};

const PROMPT: &str = "/ # ";
const DEBUG_CONTAINER: &str = "debugger-k7q2x";
const BACKSPACE: u8 = 0x7f;
const CTRL_D: u8 = 0x04;
const ERASE: &str = "\u{8} \u{8}";
const HISTORY: [(&str, &str); 2] = [
    (
        "cat /proc/1/environ | tr \"\\0\" \"\\n\" | grep STRIPE",
        "STRIPE_API_KEY=sk_live_••••••••\r\nSTRIPE_WEBHOOK_SECRET=\r\n",
    ),
    ("ls /etc/checkout", "app.yaml\r\n"),
];
const EXIT: &str = "exit";

pub fn started_note(container: &str) -> String {
    format!("Targeting container \"{container}\". Debug container: {DEBUG_CONTAINER}")
}

pub fn reply(command: &str) -> Option<String> {
    let command = command.trim();
    if command == EXIT {
        return None;
    }
    let known = HISTORY.iter().find(|(line, _)| *line == command);
    let output = match (command.is_empty(), known) {
        (true, _) => String::new(),
        (false, Some((_, output))) => (*output).to_owned(),
        (false, None) => {
            let program = command.split_whitespace().next().unwrap_or_default();
            format!("sh: {program}: not found\r\n")
        }
    };
    Some(format!("{output}{PROMPT}"))
}

fn history() -> String {
    let mut transcript = String::new();
    for (command, output) in HISTORY {
        transcript.push_str(PROMPT);
        transcript.push_str(command);
        transcript.push_str("\r\n");
        transcript.push_str(output);
    }
    transcript.push_str(PROMPT);
    transcript
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Echo(Vec<u8>),
    Exit,
}

#[derive(Debug, Default)]
pub struct LineDiscipline {
    line: Vec<u8>,
}

impl LineDiscipline {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Step> {
        let mut steps = Vec::new();
        for &byte in bytes {
            match byte {
                b'\r' | b'\n' => {
                    let command = String::from_utf8_lossy(&self.line).into_owned();
                    self.line.clear();
                    let mut echo = b"\r\n".to_vec();
                    let output = reply(&command);
                    if let Some(output) = &output {
                        echo.extend_from_slice(output.as_bytes());
                    }
                    steps.push(Step::Echo(echo));
                    if output.is_none() {
                        steps.push(Step::Exit);
                    }
                }
                BACKSPACE => {
                    if self.line.pop().is_some() {
                        steps.push(Step::Echo(ERASE.as_bytes().to_vec()));
                    }
                }
                CTRL_D => steps.push(Step::Exit),
                other => {
                    self.line.push(other);
                    steps.push(Step::Echo(vec![other]));
                }
            }
        }
        steps
    }
}

pub fn opening(debug: bool) -> String {
    if debug { history() } else { PROMPT.to_owned() }
}

pub fn run(request: &ExecRequest) {
    let debug = request.target.is_debug();
    if debug {
        let note = started_note(&request.target.container);
        if !request.output.send_blocking(ExecEvent::Started { note }) {
            return;
        }
    }
    let transcript = opening(debug).into_bytes();
    if !request.output.send_blocking(ExecEvent::Output(transcript)) {
        return;
    }
    let mut discipline = LineDiscipline::default();
    while let Some(bytes) = request.input.recv_blocking() {
        for step in discipline.feed(&bytes) {
            let event = match step {
                Step::Echo(bytes) => ExecEvent::Output(bytes),
                Step::Exit => {
                    request
                        .output
                        .send_blocking(ExecEvent::Exited { code: Some(0) });
                    return;
                }
            };
            if !request.output.send_blocking(event) {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_commands_replay_the_design_transcript() {
        assert_eq!(
            reply("ls /etc/checkout").as_deref(),
            Some("app.yaml\r\n/ # ")
        );
        assert_eq!(reply("").as_deref(), Some("/ # "));
        assert_eq!(
            reply("curl x").as_deref(),
            Some("sh: curl: not found\r\n/ # ")
        );
        assert_eq!(reply(" exit "), None);
    }

    #[test]
    fn discipline_echoes_edits_and_answers_on_enter() {
        let mut discipline = LineDiscipline::default();
        let steps = discipline.feed(b"lx\x7fs /etc/checkout\r");
        assert_eq!(steps[0], Step::Echo(b"l".to_vec()));
        assert_eq!(steps[2], Step::Echo(ERASE.as_bytes().to_vec()));
        let Some(Step::Echo(last)) = steps.last() else {
            panic!("expected an echo");
        };
        assert_eq!(String::from_utf8_lossy(last), "\r\napp.yaml\r\n/ # ");
    }

    #[test]
    fn exit_ends_the_session() {
        let mut discipline = LineDiscipline::default();
        let steps = discipline.feed(b"exit\r");
        assert_eq!(steps.last(), Some(&Step::Exit));
        assert_eq!(discipline.feed(&[CTRL_D]), [Step::Exit]);
    }

    #[test]
    fn plain_exec_opens_on_a_fresh_prompt() {
        assert_eq!(opening(false), PROMPT);
        assert_eq!(opening(true), history());
    }

    #[test]
    fn history_ends_at_a_prompt() {
        assert!(history().ends_with(PROMPT));
        assert!(history().contains("STRIPE_WEBHOOK_SECRET=\r\n"));
    }
}
