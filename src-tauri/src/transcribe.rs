use std::sync::{Arc, Mutex, OnceLock};

// Holds the PID of the running transcribe subprocess so it can be killed mid-recording
static TRANSCRIBE_PID: OnceLock<Arc<Mutex<Option<u32>>>> = OnceLock::new();

fn get_transcribe_pid() -> Arc<Mutex<Option<u32>>> {
    TRANSCRIBE_PID.get_or_init(|| Arc::new(Mutex::new(None))).clone()
}

#[tauri::command]
pub async fn stop_transcribe() {
    if let Some(pid) = get_transcribe_pid().lock().unwrap().take() {
        let _ = std::process::Command::new("kill").arg("-2").arg(pid.to_string()).status();
    }
}

#[tauri::command]
pub async fn transcribe() -> Result<String, String> {
    let child = std::process::Command::new("swift")
        .arg("/Users/marsu/Documents/Coding/Ai_Assistant/src-tauri/transcribe.swift")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    // Store PID so stop_transcribe can kill it
    *get_transcribe_pid().lock().unwrap() = Some(child.id());

    // Wait for the process — child stays local so ownership is never an issue
    let output = child.wait_with_output().map_err(|e| e.to_string())?;

    // Clear PID
    *get_transcribe_pid().lock().unwrap() = None;

    let result = String::from_utf8_lossy(&output.stdout).to_string();

    let transcription = result
        .lines()
        .find(|line| line.starts_with("RESULT: "))
        .map(|line| line.trim_start_matches("RESULT: ").to_string())
        .unwrap_or_default();

    if transcription.is_empty() {
        Err("No speech detected".to_string())
    } else {
        Ok(transcription)
    }
}
