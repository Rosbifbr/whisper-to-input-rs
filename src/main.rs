use std::io::Write;
use std::process::{Command, Stdio};

use reqwest::blocking::{Client, multipart};
use which::which;

slint::slint! {
    import { Button, VerticalBox, HorizontalBox, TextEdit } from "std-widgets.slint";
    export component MainWindow inherits Window {
        min-width: 640px;
        min-height: 480px;
        callback record_pressed <=> record.clicked;
        callback refine_pressed <=> refine.clicked;
        in-out property <string> status_text: "Idle";
        in-out property <string> transcript_text: "";
        in-out property <bool> show_refine_button: true;
        VerticalBox {
            HorizontalBox {
                status := Text {
                    text: status_text;
                    horizontal-alignment: center;
                    vertical-alignment: center;
                }
                transcript := TextEdit {
                    text: transcript_text;
                    read-only: true;
                }
            }
            HorizontalBox {
                record := Button { text: "Record"; }
                refine := Button { text: "Refine"; visible: show_refine_button; }
            }
        }
    }
}

#[derive(Eq, PartialEq)]
enum State {
    Stopped,
    Recording,
    Processing,
}

fn handle_window_state_change(window: slint::Weak<MainWindow>, state: &mut State, api_key: &str) {
    let upgraded = window.upgrade().expect("Window upgrade failed");
    match *state {
        State::Stopped => {
            Command::new("arecord")
                .args(&["-f", "cd", "-t", "wav", "-q", "/tmp/whisper_record.wav"])
                .spawn()
                .expect("Failed to start recording");
            *state = State::Recording;
            upgraded.set_status_text("Recording...".into());
        }
        State::Recording => {
            Command::new("pkill")
                .arg("arecord")
                .spawn()
                .expect("Failed to stop recording");
            *state = State::Processing;
            upgraded.set_status_text("Processing...".into());

            let client = Client::new();
            let form = multipart::Form::new()
                .file("file", "/tmp/whisper_record.wav")
                .expect("Failed to attach file")
                .text("response_format", "text")
                .text("model", "whisper-1");

            let transcript = client
                .post("https://api.openai.com/v1/audio/transcriptions")
                .header("Authorization", format!("Bearer {}", api_key))
                .multipart(form)
                .send()
                .and_then(|r| r.text())
                .unwrap_or_else(|_| "Transcription failed".into());

            copy_to_clipboard(&transcript);
            upgraded.set_transcript_text(transcript.into());
            upgraded.set_status_text("Idle".into());
            *state = State::Stopped;
        }
        _ => {}
    }
}

fn copy_to_clipboard(text: &str) {
    let mut child = Command::new("xclip")
        .args(&["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .spawn()
        .expect("Failed to spawn xclip process");

    child.stdin.as_mut()
        .expect("Failed to open xclip stdin")
        .write_all(text.as_bytes())
        .expect("Failed to write to xclip");
    
    child.wait().expect("xclip process wasn't running");
}

fn main() {
    let main_window = MainWindow::new().unwrap();
    let main_window_weak = main_window.as_weak();
    let api_key = "replace_me".to_string();

    let ask_exists = which("ask").is_ok();
    main_window.set_show_refine_button(ask_exists);

    let mut state = State::Stopped;
    main_window.on_record_pressed({
        let window = main_window_weak.clone();
        move || handle_window_state_change(window.clone(), &mut state, &api_key)
    });

    main_window.on_refine_pressed({
        let window = main_window_weak.clone();
        move || {
            let upgraded = window.upgrade().expect("Window upgrade failed");
            upgraded.set_status_text("Refining...".into());

            let transcript = upgraded.get_transcript_text().to_string();
            let prompt = format!(
                "Refine the following transcript, keeping the original style. Remove redundancies and clean up: {}",
                transcript
            );

            let output = Command::new("ask")
                .arg(prompt)
                .output()
                .expect("Failed to execute ask");

            let refined = String::from_utf8_lossy(&output.stdout).to_string();
            copy_to_clipboard(&refined);
            upgraded.set_transcript_text(refined.into());
            upgraded.set_status_text("Idle".into());

            Command::new("ask")
                .arg("-c")
                .output()
                .expect("Failed to clean up ask");
        }
    });

    main_window.run().unwrap();
}
