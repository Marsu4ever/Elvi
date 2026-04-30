use std::sync::{Arc, OnceLock, atomic::{AtomicBool, Ordering}};

use crate::get_stop_chat;

// Stops the rodio audio playback loop
static STOP_AUDIO: OnceLock<Arc<AtomicBool>> = OnceLock::new();

// Ensures only one audio track plays at a time — new audio waits for old to fully stop
static AUDIO_LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();

fn get_stop_audio() -> Arc<AtomicBool> {
    STOP_AUDIO.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

fn get_audio_lock() -> Arc<tokio::sync::Mutex<()>> {
    AUDIO_LOCK.get_or_init(|| Arc::new(tokio::sync::Mutex::new(()))).clone()
}

fn mac_builtin(text: &str) -> Result<(), String> // Fallback Audio - Dev mode - unused in Production
{
    std::process::Command::new("say")
        .arg(text)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn eleven_labs_tts(text: &str, bot: &str) -> Result<(), String> {

    // 1. Set ElevenLabs API Key from Environment
    let api_key = std::env::var("ELEVENLABS_API_KEY")
        .map_err(|_| "ELEVENLABS_API_KEY not set".to_string())?;

    // 2. Selected ElevenLabs Voices
    let voice_id = match bot 
    {
        "elvi"      => "kGOnekeZk5Zmccae0OTT",
        "alex"      => "7b1GgzhzFm98grzPUfr3",
        "samantha"  => "N3x7DJvE7NmN4ur3oV2R",
        _           => "kGOnekeZk5Zmccae0OTT", // fallback elvi
    };
    // American Assistan - natural, upbeat, conversational female voice. Swap voice_id for a different voice.
    // let voice_id = "kGOnekeZk5Zmccae0OTT"; // 1st voice - YES!

    // let voice_id = "qtRMdFaItZ1F86uQRfTL"; // Elvi - American, upbeat - Too Loud
    // let voice_id = "ASkZgqrwYoWrqNOZcH1k";  // Scarlett as a teacher
    // let voice_id = "jqcCZkN6Knx8BJ5TBdYR";  // Zara - soft, youngish voice - YES!
    // let voice_id = "56bWURjYFHyYyVf490Dp"; // Emma - Australian - Yes [OPTIONAL]
    // let voice_id = "qSeXEcewz7tA0Q0qk9fH"; // Victoria  - strong, confident 
    // let voice_id = "ZF6FPAbjXT4488VcRRnw"; // Amelia  - Bristish, reading - Yes [OPTIONAL]

    // let voice_id = "1SM7GgM6IMuvQlz2BwM3"; // Mark 
    // let voice_id = "NFG5qt843uXKj4pFvR7C"; // Adam Stone - maybe - friendly - low british - SO FAR BEST MALE
    // let voice_id = "uju3wxzG5OhpWcoi3SMy"; // Michael C. Vincent 
    // let voice_id = "Cz0K1kOv9tD8l0b5Qu53"; // Jon - ok
    // let voice_id = "NNl6r8mD7vthiJatiJt1"; // Bradford - British - almost Jarvis/Butler
    // let voice_id = "gUABw7pXQjhjt0kNFBTF"; // Andrew - authentic

    // surfer dude BqlkZRsUU8HasnGNgyKD

    // current alex vP2hb8BF0sf091jeVv07  //Surfer dude 2     // I really think he's shouting and too gravelly. 

    // let voice_id = "sf1yqvDJBhio00NGv4Hm"; //  australian - another excited one
    // alex - west coast tone - VOvAs3ikj4gydPd2Lqt5
    // alex - west coast v2 - AA8LA6B6M97AL3S3Zg6z    
    // alex - calm collected grounded - Y67NcTBPsEl0g4AUAYA2
    // alex - east coast - 7b1GgzhzFm98grzPUfr3

    // 3. Creates an HTTP Client - sort of like opening a browser
    let client = reqwest::Client::new();

    // 4. Set Configs for Elevenlabs
    let body = serde_json::json!({
        "text": text,       // text = messages in chat - IMPORTANT
        "model_id": "eleven_turbo_v2_5",
        "voice_settings": {
            "stability": 0.5,
            "similarity_boost": 0.75,
            "speed": 0.90   // (generic) Sweet spot
        }
    });

    // 5. Sends request to ElevenLabs Api
    let response = client
        .post(format!("https://api.elevenlabs.io/v1/text-to-speech/{}", voice_id))
        .header("xi-api-key", &api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    // 6. Check for API errors before decoding audio
    if !response.status().is_success() {
        let error = response.text().await.map_err(|e| e.to_string())?;
        return Err(format!("ElevenLabs error: {}", error));
    }

    let bytes = response.bytes().await.map_err(|e| e.to_string())?;

    let stop_flag = get_stop_audio();

    // 7. Run rodio in a blocking thread so the stop flag can be checked properly
    tokio::task::spawn_blocking(move || {
        let cursor = std::io::Cursor::new(bytes);
        let (_stream, stream_handle) = rodio::OutputStream::try_default()
            .map_err(|e| e.to_string())?;
        let sink = rodio::Sink::try_new(&stream_handle)
            .map_err(|e| e.to_string())?;
        let source = rodio::Decoder::new(cursor)
            .map_err(|e| e.to_string())?;
        sink.append(source);

        // 8. Checks, if should AI should stop speaking in middle of text
        while !sink.empty() // is rodio still playing AI voice to soundcard (it's not finished...)
        {
            if stop_flag.load(Ordering::Relaxed) // has someone pressed "Mute" or "Send" again, while AI voice playing
            {
                sink.stop();    // tell rodio to stop (= no more playing AI voice)
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Ok::<(), String>(())
    }).await.map_err(|e| e.to_string())??;  // Double unwrap... I don't like this.

    Ok(())
}

#[tauri::command]
pub async fn speak(text: String, bot: String) -> Result<(), String> {
    // Signal any currently playing audio to stop
    get_stop_audio().store(true, Ordering::Relaxed);

    // Wait for the previous audio to fully release before starting ours
    // (the lock is held for the duration of playback)
    let audio_lock = get_audio_lock();
    let _guard = audio_lock.lock().await;

    // We now own the audio channel — safe to reset and play
    get_stop_audio().store(false, Ordering::Relaxed);

    //  A. say (Mac builtin - Text to Speech - fast and free)
    //mac_builtin(&text)?; // Backup voice in case ElevenLabs fails

    // C. ElevenLabs (better native voice, paid-tier)
    eleven_labs_tts(&text, &bot).await?;

    Ok(())
}

#[tauri::command]
pub fn stop_speaking() {
    get_stop_audio().store(true, Ordering::Relaxed);
    get_stop_chat().store(true, Ordering::Relaxed);
}
