//! Calls to the agent runtime's `voice` commands; it owns every credential and the saved choice.
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

/// What went wrong, and the step the desktop should offer.
#[derive(Debug)]
pub(super) struct Refusal {
    pub message: String,
    /// `choose-engine`, `needs-key`, `bad-key`, or another code that has no special step.
    pub code: String,
}

fn agent() -> Command {
    // Development override, so the preview on a workstation can reach a checkout's runtime.
    Command::new(std::env::var_os("AGX_AGENT_BIN").unwrap_or_else(|| "/usr/bin/agent".into()))
}

fn unavailable() -> Refusal {
    Refusal { message: "Voice typing needs the ANGI agent runtime, which did not answer.".into(), code: "failed".into() }
}

fn interpret(answer: Value) -> Result<Value, Refusal> {
    match answer["error"].as_str() {
        Some(message) => Err(Refusal { message: message.into(), code: answer["code"].as_str().unwrap_or("failed").into() }),
        None => Ok(answer),
    }
}

fn parse(stdout: &[u8]) -> Result<Value, Refusal> {
    interpret(serde_json::from_slice(stdout).map_err(|_| unavailable())?)
}

/// Run `agent voice ARGS`, giving it `input` on standard input.
fn run(args: &[&str], input: &[u8]) -> Result<Value, Refusal> {
    let mut child = agent()
        .arg("voice")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| unavailable())?;
    if let Some(mut stdin) = child.stdin.take() {
        // A runtime that exits early closes the pipe; its answer still follows.
        let _ = stdin.write_all(input);
    }
    let output = child.wait_with_output().map_err(|_| unavailable())?;
    parse(&output.stdout)
}

pub(super) fn transcribe(wav: &[u8]) -> Result<String, Refusal> {
    run(&["transcribe"], wav).map(|answer| answer["text"].as_str().unwrap_or("").trim().to_string())
}

/// This device, the practical models and the saved choice, for the setup overlay.
pub(super) fn options() -> Result<Value, Refusal> {
    run(&["options"], b"")
}

/// Save the engine to use: "openai", or "local" with a downloaded model.
pub(super) fn choose(kind: &str, model: Option<&str>) -> Result<(), Refusal> {
    let mut args = vec!["choose", kind];
    args.extend(model);
    run(&args, b"").map(|_| ())
}

/// Save an OpenAI API key for voice. The key goes on standard input, never on a command line.
pub(super) fn save_key(key: &str) -> Result<(), Refusal> {
    run(&["key"], format!("{}\n", key.trim()).as_bytes()).map(|_| ())
}

/// Download a model, reporting progress from 0 to 1 as it arrives.
pub(super) fn install(model: &str, mut progress: impl FnMut(f32)) -> Result<(), Refusal> {
    let mut child = agent()
        .args(["voice", "install", model])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| unavailable())?;
    let mut outcome = Err(unavailable());
    for line in BufReader::new(child.stdout.take().ok_or_else(unavailable)?).lines().map_while(Result::ok) {
        let Ok(update) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(fraction) = update["progress"].as_f64() {
            progress(fraction as f32);
        } else {
            outcome = interpret(update).map(|_| ());
        }
    }
    let _ = child.wait();
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_become_text_or_a_refusal_with_its_code() {
        assert_eq!(parse(br#"{"text":"hi"}"#).unwrap()["text"], "hi");
        let refused = parse(br#"{"error":"Choose","code":"choose-engine"}"#).unwrap_err();
        assert_eq!((refused.message.as_str(), refused.code.as_str()), ("Choose", "choose-engine"));
        assert_eq!(parse(b"").unwrap_err().code, "failed");
    }
}
