//! Needs the model and a usable GPU (or `OXIVOX_TEST_CPU=1`): `cargo test --release -- --ignored`.

use std::process::Command;

#[test]
#[ignore = "needs the whisper model and a GPU"]
fn transcribes_a_synthetic_sentence() {
    let wav = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/warmup.wav");
    let mut command = Command::new(env!("CARGO_BIN_EXE_oxivox"));
    command.args(["transcribe", wav]);
    if std::env::var_os("OXIVOX_TEST_CPU").is_some() {
        command.arg("--cpu");
    }
    let output = command.output().expect("running oxivox");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
    assert!(text.contains("quick brown fox") && text.contains("lazy dog"), "got {text:?}");
}
