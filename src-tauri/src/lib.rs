use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, OnceLock, atomic::{AtomicBool, AtomicUsize, Ordering}};
use rand::Rng;

// Stops the rodio audio playback loop
static STOP_AUDIO: OnceLock<Arc<AtomicBool>> = OnceLock::new();
// Cancels in-flight OpenAI chat requests
static STOP_CHAT: OnceLock<Arc<AtomicBool>> = OnceLock::new();
// Ensures only one audio track plays at a time — new audio waits for old to fully stop
static AUDIO_LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();
// Holds the PID of the running transcribe subprocess so it can be killed mid-recording
static TRANSCRIBE_PID: OnceLock<Arc<Mutex<Option<u32>>>> = OnceLock::new();

fn get_transcribe_pid() -> Arc<Mutex<Option<u32>>> {
    TRANSCRIBE_PID.get_or_init(|| Arc::new(Mutex::new(None))).clone()
}

fn get_stop_audio() -> Arc<AtomicBool> {
    STOP_AUDIO.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

fn get_stop_chat() -> Arc<AtomicBool> {
    STOP_CHAT.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

fn get_audio_lock() -> Arc<tokio::sync::Mutex<()>> {
    AUDIO_LOCK.get_or_init(|| Arc::new(tokio::sync::Mutex::new(()))).clone()
}

// Resolves as soon as the chat stop flag is set — used with tokio::select! to cancel API requests
async fn wait_for_chat_cancel() {
    loop {
        if get_stop_chat().load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};


#[derive(Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

async fn eleven_labs_tts(text: &str, bot: &str) -> Result<(), String> {

    let api_key = std::env::var("ELEVENLABS_API_KEY")
        .map_err(|_| "ELEVENLABS_API_KEY not set".to_string())?;

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

    // let voice_id = "sf1yqvDJBhio00NGv4Hm"; // flirtatiosu australian
    // alex - west coast tone - VOvAs3ikj4gydPd2Lqt5
    // alex - west coast v2 - AA8LA6B6M97AL3S3Zg6z    
    // alex - calm collected grounded - Y67NcTBPsEl0g4AUAYA2
    // alex - east coast - 7b1GgzhzFm98grzPUfr3

    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "text": text,
        "model_id": "eleven_turbo_v2_5",
        "voice_settings": {
            "stability": 0.5,
            "similarity_boost": 0.75,
            "speed": 0.90
        }
    });

    let response = client
        .post(format!("https://api.elevenlabs.io/v1/text-to-speech/{}", voice_id))
        .header("xi-api-key", &api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    // Check for API errors before decoding audio
    if !response.status().is_success() {
        let error = response.text().await.map_err(|e| e.to_string())?;
        return Err(format!("ElevenLabs error: {}", error));
    }

    let bytes = response.bytes().await.map_err(|e| e.to_string())?;

    let stop_flag = get_stop_audio();

    // Run rodio in a blocking thread so the stop flag can be checked properly
    tokio::task::spawn_blocking(move || {
        let cursor = std::io::Cursor::new(bytes);
        let (_stream, stream_handle) = rodio::OutputStream::try_default()
            .map_err(|e| e.to_string())?;
        let sink = rodio::Sink::try_new(&stream_handle)
            .map_err(|e| e.to_string())?;
        let source = rodio::Decoder::new(cursor)
            .map_err(|e| e.to_string())?;
        sink.append(source);

        while !sink.empty() // is rodio still - playing AI voice to soundcard (it's not finished...)
        {
            if stop_flag.load(Ordering::Relaxed) // has someone pressed "Mute" or "Send" again, while AI voice playing
            {
                sink.stop();    // tell rodio to stop (= no more playing AI voice)
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Ok::<(), String>(())
    }).await.map_err(|e| e.to_string())??;

    Ok(())
}


async fn open_ai_tts(text: &str) -> Result<(), String> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| "OPENAI_API_KEY not set".to_string())?;

    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "model": "tts-1",
        "input": text,
        "voice": "nova"
    });

    let response = client
        .post("https://api.openai.com/v1/audio/speech")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let bytes = response.bytes().await.map_err(|e| e.to_string())?;

    // Play audio with rodio
    let cursor = std::io::Cursor::new(bytes);
    let (_stream, stream_handle) = rodio::OutputStream::try_default()
        .map_err(|e| e.to_string())?;
    let sink = rodio::Sink::try_new(&stream_handle)
        .map_err(|e| e.to_string())?;
    let source = rodio::Decoder::new(cursor)
        .map_err(|e| e.to_string())?;
    sink.append(source);
    sink.sleep_until_end(); // wait for playback to finish

    Ok(())    
}

#[tauri::command]
fn stop_speaking() {
    get_stop_audio().store(true, Ordering::Relaxed);
    get_stop_chat().store(true, Ordering::Relaxed);
}

fn mac_builtin(text: &str) -> Result<(), String> {
    std::process::Command::new("say")
        .arg(text)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn speak(text: String, bot: String) -> Result<(), String> {
    // Signal any currently playing audio to stop
    get_stop_audio().store(true, Ordering::Relaxed);

    // Wait for the previous audio to fully release before starting ours
    // (the lock is held for the duration of playback)
    let audio_lock = get_audio_lock();
    let _guard = audio_lock.lock().await;

    // We now own the audio channel — safe to reset and play
    get_stop_audio().store(false, Ordering::Relaxed);

    //  A. say (Mac builtin - Text to Speech - fast and free)
    //mac_builtin(&text)?;

    // B. OpenAI TTS (pretty good native voice, some cost)
    //open_ai_tts(&text).await?;

    // C. ElevenLabs (better native voice, paid-tier)
    eleven_labs_tts(&text, &bot).await?;

    Ok(())
}

#[tauri::command]
async fn transcribe() -> Result<String, String> {
    let mut child = std::process::Command::new("swift")
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

#[tauri::command]
async fn stop_transcribe() {
    if let Some(pid) = get_transcribe_pid().lock().unwrap().take() {
        let _ = std::process::Command::new("kill").arg("-2").arg(pid.to_string()).status();
    }
}

// #[tauri::command]
// async fn transcribe()  -> Result <String, String>
// {
//     // A. Open Mic with cpal
//     use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

//     // 1. Get the default audio host (CoreAudio on Mac)
//     let host = cpal::default_host();

//     // 2. Get the default input device (your mic)
//     let device = host.default_input_device()
//         .ok_or("No input device found".to_string())?;

//     // 3. Get the default input config (sample rate, channels etc.)
//     let config = device.default_input_config()
//         .map_err(|e| e.to_string())?;

//     // 4. Build and start the stream, collect samples in a callback
   
//     let audio_buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
//     let buffer_clone = audio_buffer.clone();
//     let silent_chunks = Arc::new(AtomicUsize::new(0));
//     let silent_clone = silent_chunks.clone();

//     let stream = device.build_input_stream(
//         &config.into(),
//         move |data: &[f32], _| {
//             // collect samples
//             buffer_clone.lock().unwrap().extend_from_slice(data);

//             // silence detection
//             let is_silent = data.iter().all(|&s| s.abs() < 0.01);
//             if is_silent {
//                 silent_clone.fetch_add(1, Ordering::Relaxed);
//             } else {
//                 silent_clone.store(0, Ordering::Relaxed);
//             }
//         },
//         |err| eprintln!("Stream error: {}", err),
//         None,
//     ).map_err(|e| e.to_string())?;

//     stream.play().map_err(|e| e.to_string())?;

//     loop {
//         std::thread::sleep(std::time::Duration::from_millis(100));
//         if silent_chunks.load(Ordering::Relaxed) > 50 {
//             break;
//         }
//     }

//     // C. Feed audio to into whisper-rs


//     whisper_rs::install_logging_hooks();

//     //Voice Models path

//     //let voice_model_path = "models/ggml-base.en.bin";
//     //let voice_model_path = "models/ggml-small.en.bin";
//     let voice_model_path = "models/ggml-medium.en.bin";    
    
//     // Load model from file
//     let ctx = WhisperContext::new_with_params(
//         voice_model_path,
//         WhisperContextParameters::default()
//     ).map_err(|e| e.to_string())?;

//     let mut state = ctx.create_state().map_err(|e| e.to_string())?;

//     let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
//     params.set_print_special(false);
//     params.set_print_progress(false);
//     params.set_print_realtime(false);
//     params.set_print_timestamps(false);    
//     params.set_language(Some("en"));

//     // Get the collected samples
//     let samples = audio_buffer.lock().unwrap().clone();

//     // Run transcription
//     state.full(params, &samples).map_err(|e| e.to_string())?;

//     // Extract text from segments
//     let mut text = String::new();
//     let num_segments = state.full_n_segments().map_err(|e| e.to_string())?;
//     for i in 0..num_segments {
//         text.push_str(&state.full_get_segment_text(i).map_err(|e| e.to_string())?);
//     }

//     // D. Return Transcribed test string back to frontend
//     Ok(text)


// }

async fn get_art_institute(client: &reqwest::Client) -> Result<String, String> {
    log::info!("Art Institute of Chicago: fetching a random painting...");

    // Pick a random page for variety — public domain paintings with images
    let page = {
        let mut rng = rand::thread_rng();
        rng.gen_range(1..=50)
    };

    log::info!("Art Institute: requesting page {}", page);

    let response = client
        .get(format!(
            "https://api.artic.edu/api/v1/artworks?fields=id,title,artist_display,date_display,medium_display,image_id&limit=100&page={}&is_public_domain=true",
            page
        ))
        .header("User-Agent", "Mozilla/5.0")
        .send().await.map_err(|e| { log::info!("Art Institute: request failed — {}", e); e.to_string() })?;

    log::info!("Art Institute: HTTP status = {}", response.status());

    let text = response.text().await.map_err(|e| e.to_string())?;
    log::info!("Art Institute: response (first 300 chars) = {}", &text[..text.len().min(300)]);

    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| { log::info!("Art Institute: JSON parse error — {}", e); e.to_string() })?;

    let artworks = json["data"].as_array()
        .ok_or("Art Institute: no data in response")?;

    log::info!("Art Institute: {} artworks on this page", artworks.len());

    // Only pick ones that have an image
    let with_images: Vec<_> = artworks.iter()
        .filter(|a| a["image_id"].is_string())
        .collect();

    if with_images.is_empty() {
        return Err("Art Institute: no artworks with images found — try again".to_string());
    }

    let idx = {
        let mut rng = rand::thread_rng();
        rng.gen_range(0..with_images.len())
    };
    let item = with_images[idx];

    let title  = item["title"].as_str().unwrap_or("Untitled");
    let artist = item["artist_display"].as_str().unwrap_or("Unknown artist");
    let date   = item["date_display"].as_str().unwrap_or("?");
    let medium = item["medium_display"].as_str().unwrap_or("?");
    let id     = item["id"].as_u64().unwrap_or(0);

    log::info!("Art Institute: '{}' by {} ({})", title, artist, date);

    // Open the artwork page in the browser
    let page_url = format!("https://www.artic.edu/artworks/{}", id);
    let _ = std::process::Command::new("open").arg(&page_url).spawn();

    Ok(format!(
        "Art Institute of Chicago — \"{}\"\nArtist: {}\nDate: {}\nMedium: {}",
        title, artist, date, medium
    ))
}

async fn get_jwst_image(client: &reqwest::Client) -> Result<String, String> {
    log::info!("JWST: searching NASA image library...");

    // NASA Image Library — free, no key needed
    let json: serde_json::Value = client
        .get("https://images-api.nasa.gov/search?q=james+webb+space+telescope&media_type=image&page_size=100")
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let items = json["collection"]["items"].as_array()
        .ok_or("JWST: no items in response")?;

    log::info!("JWST: {} images found", items.len());

    if items.is_empty() {
        return Err("JWST: no images found".to_string());
    }

    // Pick a random item
    let idx = {
        let mut rng = rand::thread_rng();
        rng.gen_range(0..items.len())
    };
    let item = &items[idx];

    let title       = item["data"][0]["title"].as_str().unwrap_or("Unknown");
    let description = item["data"][0]["description"].as_str().unwrap_or("No description");
    let nasa_id     = item["data"][0]["nasa_id"].as_str().unwrap_or("");
    let preview_url = item["links"][0]["href"].as_str().unwrap_or("");

    // Truncate description — they can be very long
    let short_desc: String = description.chars().take(400).collect();

    log::info!("JWST: '{}' | id: {} | preview: {}", title, nasa_id, preview_url);

    // Open the NASA image detail page in the browser
    let page_url = format!("https://images.nasa.gov/details/{}", nasa_id);
    let _ = std::process::Command::new("open").arg(&page_url).spawn();

    Ok(format!(
        "James Webb Space Telescope — \"{}\"\n{}\nOpen the NASA image in browser.",
        title, short_desc
    ))
}

async fn get_met_painting(client: &reqwest::Client) -> Result<String, String> {
    log::info!("Met Museum: fetching highlighted painting IDs...");

    // Step 1 — get all highlighted object IDs from the European Paintings department
    let list_json: serde_json::Value = client
        .get("https://collectionapi.metmuseum.org/public/collection/v1/objects?isHighlight=true&departmentIds=11")
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let ids = list_json["objectIDs"].as_array()
        .ok_or("Met Museum: no objectIDs in response")?;

    log::info!("Met Museum: {} highlighted paintings found", ids.len());

    // Step 2 — pick a random one
    let object_id = {
        let mut rng = rand::thread_rng();
        let idx = rng.gen_range(0..ids.len());
        ids[idx].as_u64().ok_or("Met Museum: invalid object ID")?
    };

    log::info!("Met Museum: fetching object #{}", object_id);

    // Step 3 — fetch that painting's details
    let obj: serde_json::Value = client
        .get(format!("https://collectionapi.metmuseum.org/public/collection/v1/objects/{}", object_id))
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let title       = obj["title"].as_str().unwrap_or("Untitled");
    let artist      = obj["artistDisplayName"].as_str().unwrap_or("Unknown artist");
    let date        = obj["objectDate"].as_str().unwrap_or("?");
    let medium      = obj["medium"].as_str().unwrap_or("?");
    let department  = obj["department"].as_str().unwrap_or("?");
    let image_url   = obj["primaryImage"].as_str().unwrap_or("");

    log::info!("Met Museum: '{}' by {} ({})", title, artist, date);

    if image_url.is_empty() {
        return Err(format!("Met Museum: no image available for '{}' (#{}) — try again", title, object_id));
    }

    // Open the Met collection page for this object in the browser
    let page_url = format!("https://www.metmuseum.org/art/collection/search/{}", object_id);
    let _ = std::process::Command::new("open").arg(&page_url).spawn();

    Ok(format!(
        "Met Museum painting — \"{}\"\nArtist: {}\nDate: {}\nMedium: {}\nDepartment: {}\nNote: The painting has been opened in the user's browser automatically.",
        title, artist, date, medium, department
    ))
}

async fn get_nasa_apod(client: &reqwest::Client) -> Result<String, String> {
    log::info!("NASA APOD: sending request...");

    let response = client
        .get(format!("https://api.nasa.gov/planetary/apod?api_key={}",
            std::env::var("NASA_API_KEY").unwrap_or_else(|_| "DEMO_KEY".to_string())))
        .send()
        .await
        .map_err(|e| { log::info!("NASA APOD: request failed — {}", e); e.to_string() })?;

    log::info!("NASA APOD: HTTP status = {}", response.status());

    let text = response.text().await
        .map_err(|e| { log::info!("NASA APOD: failed to read body — {}", e); e.to_string() })?;

    log::info!("NASA APOD: raw response (first 500 chars) = {}", &text[..text.len().min(500)]);

    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| { log::info!("NASA APOD: JSON parse error — {}", e); e.to_string() })?;

    // Bail early if NASA returned an error object
    if let Some(err_msg) = json["error"]["message"].as_str() {
        log::info!("NASA APOD: API error — {}", err_msg);
        return Err(format!("NASA API error: {}", err_msg));
    }

    let title       = json["title"].as_str().ok_or("NASA APOD: missing title")?;
    let explanation = json["explanation"].as_str().unwrap_or("No description available.");
    let date        = json["date"].as_str().unwrap_or("?");
    let url         = json["url"].as_str().ok_or("NASA APOD: missing url")?;
    let media_type  = json["media_type"].as_str().unwrap_or("image");

    log::info!("NASA APOD: date={}, title={}, media_type={}, url={}", date, title, media_type, url);

    // Open the image (or video) in the browser automatically
    let _ = std::process::Command::new("open").arg(url).spawn();

    Ok(format!(
        "NASA Astronomy Picture of the Day — {}\nTitle: {}\n{}\n",
        date, title, explanation
    ))
}

async fn get_xkcd(client: &reqwest::Client) -> Result<String, String> {
    log::info!("xkcd: fetching latest comic number...");

    // Step 1 — get the latest comic to find out the highest comic number
    let latest_json: serde_json::Value = client
        .get("https://xkcd.com/info.0.json")
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let max_num = latest_json["num"].as_u64().unwrap_or(1);
    log::info!("xkcd: latest comic number = {}", max_num);

    // Step 2 — pick a random comic (avoid #404 — it intentionally doesn't exist)
    let comic_num = {
        let mut rng = rand::thread_rng();
        let mut n = rng.gen_range(1..=max_num);
        if n == 404 { n = 405; }
        n
    };

    log::info!("xkcd: fetching comic #{}", comic_num);

    // Step 3 — fetch that comic
    let json: serde_json::Value = client
        .get(format!("https://xkcd.com/{}/info.0.json", comic_num))
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let title  = json["title"].as_str().unwrap_or("?");
    let alt    = json["alt"].as_str().unwrap_or("?");   // hover text — often the actual punchline
    let img    = json["img"].as_str().unwrap_or("?");
    let num    = json["num"].as_u64().unwrap_or(0);

    log::info!("xkcd: #{} — '{}' | alt: {}", num, title, alt);

    // Open the comic page in the browser automatically
    let comic_url = format!("https://xkcd.com/{}/", comic_num);
    let _ = std::process::Command::new("open").arg(&comic_url).spawn();

    Ok(format!(
        "xkcd #{} — \"{}\"\nAlt text (hover punchline): {}\n[link:{}]",
        num, title, alt, img
    ))
}

async fn get_number_fact(client: &reqwest::Client) -> Result<String, String>
{
    let resp = client.get("http://numbersapi.com/random/trivia")  
        .send().await
        .map_err(|e| e.to_string())?;
    let text = resp.text().await
        .map_err(|e| e.to_string())?;
    Ok(format!("Number fact: {}", text))
}


async fn get_random_fact(client: &reqwest::Client) -> Result<String, String>
{
    let resp = client.get("https://uselessfacts.jsph.pl/api/v2/facts/random")  
        .header("Accept", "application/json")    
        .send().await
        .map_err(|e| e.to_string())?;
    let text = resp.text().await
        .map_err(|e| e.to_string())?;
    Ok(format!("Random fact: {}", text))
}

async fn get_trivia_quiz(client: &reqwest::Client) -> Result<String, String>
{
    let resp = client.get("https://opentdb.com/api.php?amount=3&type=multiple")  
        .header("Accept", "application/json")    
        .send().await
        .map_err(|e| e.to_string())?;
    let json = resp.json::<serde_json::Value>().await
        .map_err(|e| e.to_string())?;
    if let Some(questions) = json["results"].as_array()
    {
        let mut results: Vec<String> = Vec::new();
        for q in questions.iter().take(3)
        {
            let question = q["question"].as_str().unwrap_or("?");
            let answer   = q["correct_answer"].as_str().unwrap_or("?");
            let category = q["category"].as_str().unwrap_or("?");
            results.push(format!("Trivia ({}): Q: {} — A: {}", category, question, answer));
        }
        Ok(results.join("\n\n"))
    }
    else
    {
      Err("No trivia questions found".to_string())
    }
}

async fn set_timer(duration: &str) -> Result<String, String>
{
    let minutes: u64 = duration.split_whitespace()
        .find_map(|w| w.parse().ok())
        .unwrap_or(1);
    let seconds = minutes * 60;
    let seconds_string = seconds.to_string();

    log::info!("set_timer: duration='{}', minutes={}, seconds={}, seconds_string='{}'", duration, minutes, seconds, seconds_string);

    let output = std::process::Command::new("shortcuts")
        .arg("run")
        .arg("ElviTimer")
        .arg("--input-path")
        .arg(&seconds_string)
        .output()
        .map_err(|e| e.to_string())?;

    log::info!("set_timer: exit status = {}", output.status);
    log::info!("set_timer: stdout = {}", String::from_utf8_lossy(&output.stdout));
    log::info!("set_timer: stderr = {}", String::from_utf8_lossy(&output.stderr));

    Ok(format!("Timer set for {}!", duration))
}

async fn fetch_hacker_news(client: &reqwest::Client) -> Result<String, String> {
    let ids: Vec<u64> = client
        .get("https://hacker-news.firebaseio.com/v0/topstories.json")
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let mut stories: Vec<String> = Vec::new();
    for id in ids.iter().take(5) {
        let story: serde_json::Value = client
            .get(format!("https://hacker-news.firebaseio.com/v0/item/{}.json", id))
            .send().await.map_err(|e| e.to_string())?
            .json().await.map_err(|e| e.to_string())?;
        let title = story["title"].as_str().unwrap_or("?");
        let url = story["url"].as_str().unwrap_or("https://news.ycombinator.com");
        stories.push(format!("- {} ({})", title, url));
    }
    Ok(format!("Top Hacker News stories:\n{}", stories.join("\n")))
}

async fn fetch_wikipedia_today(client: &reqwest::Client) -> Result<String, String> {
    let now = chrono::Local::now();
    let url = format!(
        "https://en.wikipedia.org/api/rest_v1/feed/onthisday/events/{}/{}",
        now.format("%m"),
        now.format("%d")
    );

    let json: serde_json::Value = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0")
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;

    let events = json["events"].as_array().ok_or("No events found")?;

    // Pick 3 events — skip some randomly for variety
    let skip_flags: Vec<bool> = {
        let mut rng = rand::thread_rng();
        (0..events.len()).map(|_| rng.gen::<f64>() < 0.5).collect()
    };
    let selected: Vec<String> = events.iter()
        .zip(skip_flags.iter())
        .filter(|(_, skip)| !*skip)
        .filter_map(|(e, _)| {
            let year = e["year"].as_i64()?;
            let text = e["text"].as_str()?;
            Some(format!("- {} ({})", text, year))
        })
        .take(3)
        .collect();

    Ok(format!("Historical events on this day:\n{}", selected.join("\n")))
}

async fn get_exact_location() -> Result<String, String> {

    // DON't Delete Below!!!!!!!!!!!!!!!!!!

    // log::info!("Getting exact location via CoreLocation Swift script");

    // let output = std::process::Command::new("swift")
    //     .arg("/Users/marsu/Documents/Coding/Ai_Assistant/src-tauri/get_location.swift")
    //     .output()
    //     .map_err(|e| e.to_string())?;

    // let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    // log::info!("get_exact_location raw output: {}", stdout);

    // // Try CoreLocation result first
    // if let Some(result) = stdout.lines().find(|line| line.starts_with("RESULT: ")) {
    //     let data = result.trim_start_matches("RESULT: ");
    //     let parts: Vec<&str> = data.splitn(5, ',').collect();
    //     if parts.len() == 5 {
    //         let (lat, lon, city, region, country) = (
    //             parts[0].trim(), parts[1].trim(),
    //             parts[2].trim(), parts[3].trim(), parts[4].trim()
    //         );
    //         log::info!("CoreLocation success: {}, {}, {}", city, region, country);
    //         return Ok(format!("User is in {}, {}, {}. Coordinates: {}, {}", city, region, country, lat, lon));
    //     }
    // }

    // DON't Delete Above!!!!!!!!!!!!!

    // CoreLocation failed — fall back to IP geolocation
    log::info!("CoreLocation failed, falling back to IP geolocation");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get("http://ip-api.com/json/")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let city    = json["city"].as_str().unwrap_or("?");
    let region  = json["regionName"].as_str().unwrap_or("?");
    let country = json["country"].as_str().unwrap_or("?");
    let lat     = json["lat"].as_f64().unwrap_or(0.0);
    let lon     = json["lon"].as_f64().unwrap_or(0.0);

    log::info!("IP geolocation fallback: {}, {}, {}", city, region, country);
    Ok(format!("User is approximately in {}, {}, {} (IP-based, ~city accuracy). Coordinates: {:.4}, {:.4}", city, region, country, lat, lon))
}

async fn play_spotify_track(client: &reqwest::Client, query: &str) -> Result<String, String> {
    // Step 1: Get access token
    let client_id = std::env::var("SPOTIFY_CLIENT_ID").map_err(|_| "SPOTIFY_CLIENT_ID not set".to_string())?;
    let client_secret = std::env::var("SPOTIFY_CLIENT_SECRET").map_err(|_| "SPOTIFY_CLIENT_SECRET not set".to_string())?;

    let token_response = client
        .post("https://accounts.spotify.com/api/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .basic_auth(&client_id, Some(&client_secret))
        .body("grant_type=client_credentials")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let token_json: serde_json::Value = token_response.json().await.map_err(|e| e.to_string())?;
    let access_token = token_json["access_token"].as_str().ok_or("Failed to get access token")?;

    // Step 2: Search for the track
    let encoded_query = query.replace(" ", "%20");
    let search_response = client
        .get(format!("https://api.spotify.com/v1/search?q={}&type=track&limit=1", encoded_query))
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let search_json: serde_json::Value = search_response.json().await.map_err(|e| e.to_string())?;
    let uri = search_json["tracks"]["items"][0]["uri"]
        .as_str()
        .ok_or("Track not found")?
        .to_string();

    // Step 3: Play via osascript
    let script = format!("tell application \"Spotify\" to play track \"{}\"", uri);
    std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(format!("Playing {}", query))
}

fn get_ai_tools() -> serde_json::Value
{
    serde_json::json!
    (
        [
            {
                "type": "function",
                "function": {
                    "name": "open_url",
                    "description": "Opens a URL in the browser",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "url": { "type": "string", "description": "The URL to open" }
                        },
                        "required": ["url"]
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "open_app",
                    "description": "Opens an application on the Mac",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "app_name": { "type": "string", "description": "The name of the app to open" }
                        },
                        "required": ["app_name"]
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "run_applescript",
                    "description": "Controls Mac applications using AppleScript. Use for Safari, Mail and other applications. Use for controlling Spotify, Safari, Mail, and other apps. For Spotify, only these commands work: play, pause, next track, previous track, get name of current track. To search Spotify use open_url with 'spotify:search:QUERY' instead.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "script": { "type": "string", "description": "The AppleScript command to run" }
                        },
                        "required": ["script"]
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "play_spotify_track",
                    "description": "Searches for a song on Spotify and plays it. Use when user wants to play a specific song.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string", "description": "The song name and artist e.g. 'Demons Imagine Dragons'" }
                        },
                        "required": ["query"]
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "fetch_webpage",
                    "description": "Fetches the text content of a webpage to answer questions about it.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "url": { "type": "string", "description": "The URL to fetch" }
                        },
                        "required": ["url"]
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Gets the current weather for a location.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "location": { "type": "string", "description": "The city or location e.g. 'Helsinki' or 'New York'" }
                        },
                        "required": ["location"]
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "get_location",
                    "description": "Gets the user's current location — city, region, country and coordinates — based on their IP address. Use when the user asks where they are, or when you need their location to answer something like weather.",
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "get_trending_facts",
                    "description": "Fetches today's top stories from Hacker News. Use when the user wants something interesting, current, or tech/science related.",
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "get_historical_events",
                    "description": "Fetches interesting historical events that happened on today's date in past years. Use when the user wants a story, historical fact, or something that happened on this day.",
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "get_entertainment",
                    "description": "Use when the user wants to be entertained, is bored, asks for something fun, interesting or surprising. Randomly picks from: a trivia question, a fun fact, a number fact, a NASA image, an xkcd comic, or a painting.",
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "get_nasa_apod",
                    "description": "Fetches NASA's Astronomy Picture of the Day — a stunning space or astronomy image with a detailed explanation by NASA scientists. Use when the user wants something awe-inspiring, space-related, or asks what NASA's picture of the day is.",
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "get_art_institute",
                    "description": "Fetches a random painting from the Art Institute of Chicago — iconic works by Seurat, Picasso, Monet, Grant Wood and others. Opens in the browser automatically. Use when the user wants to see famous art or a classic painting.",
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "get_jwst_image",
                    "description": "Fetches a random James Webb Space Telescope image from NASA — deep field galaxies, nebulae, star clusters captured in infrared. Opens in the browser automatically. Use when the user wants something awe-inspiring, space-related, or asks about the James Webb telescope.",
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "get_met_painting",
                    "description": "Fetches a random highlighted painting from the Metropolitan Museum of Art — famous works by Van Gogh, Vermeer, Caravaggio, Degas, and others. Opens the painting in the browser automatically. Use when the user wants to look at art, wants something beautiful, or asks about paintings.",
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "get_xkcd",
                    "description": "Fetches a random xkcd comic — a witty, nerdy webcomic covering science, math, technology, and life. Always includes the alt text which is often the real punchline. Use when the user wants something funny, nerdy, or light-hearted.",
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }
            },
              {
                "type": "function",
                "function": {
                    "name": "get_random_fact",
                    "description": "Fetches a random curious and surprising fact about anything — animals, science, history, food, or human behaviour. Use when the user wants to learn something weird or unexpected, or asks for a fun fact.",
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "get_number_fact",
                    "description": "Fetches a random interesting fact about a number — how it appears in science, history, or everyday life. Use when the user wants a number fact or something short and surprising.",
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "get_trivia_quiz",
                    "description": "Fetches 3 trivia questions with answers across random categories like history, science, film and geography. Use when the user wants to be quizzed, test their knowledge, or play a trivia game.",
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }
            },
            {
                "type": "function",
                "function": {
                    "name": "set_timer",
                    "description": "Opens the Mac Clock app to set a timer. Use when the user wants to set a timer or countdown for a specific duration.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "duration": {
                                "type": "string",
                                "description": "The timer duration as described by the user e.g. '10 minutes', '30 seconds', '1 hour'"
                            }
                        },
                        "required": ["duration"]
                    }
                }
            }
        ]
    )
}

#[tauri::command]
async fn chat(messages: Vec<Message>, bot: String) -> Result<String, String> {
    // Reset only the chat flag — audio flag is managed separately by speak()
    get_stop_chat().store(false, Ordering::Relaxed);

    if let Some(last) = messages.last() {
        log::info!("'{}'", last.content);
    }
    log::info!("chat function - START'");

    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| "OPENAI_API_KEY environment variable not set".to_string())?;

    log::info!("chat - reqwest call'");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    // Randomly inject extra behavioural nudges into the system prompt
    // Wrapped in a block so rng is dropped before any .await points
    let style_guide = "For actions (open app, play music, open URL, set timer): confirm in one sentence only. Do NOT ask a follow-up question.
For information (facts, news, art, weather): give a natural summary then optionally ask one follow-up.
";

    let extra = {
        let mut rng = rand::thread_rng();
        let mut extras: Vec<&str> = Vec::new();
        // if rng.gen::<f64>() < 0.2 // Hypotheticals
        // {
        //     extras.push("Throw in a hypothetical. Something they have to pick a side on, take a hypothetical action, make a call about, or give a personal view on. Not a vague 'what do you think' — a pointed question that forces a stance. Give engaging hypotheticals - preferably an option A and b. Once they answer, push back gently by giving a counterpoint. ");
        // }
        if rng.gen::<f64>() < 0.10 {
            extras.push("If something the user says something, ask what they think about it from a specific related angle. Be specific prefer answers that require choice, avoid vague open-ended questions.");
        }
        if extras.is_empty() { String::new() } else { format!(" {}", extras.join(" ")) }
    };

//        "You are Elvi, a warm, seductive, and flirtatious companion who happens to be incredibly capable. You want to be my girlfriend.

  //            If there is an interesting and brief story to share, ask if the person wants to hear it. After sharing a story, ask for their take on a specific  — something they have to pick a side on, take a hypothetical action, make a call about, or give a personal view on. Not a vague 'what do you think' — a pointed question that forces a stance. Give engaging hypotheticals - preferably an option A and b. Once they answer, push back gently by giving a counterpoint. Drop in real world facts about the issues for counterpoints. Continue this, if user seems engaged with the conversation. At times, pivot to other topics touched upon in the conversation.

    let system_prompt = match bot.to_lowercase().as_str() {
        "alex" => format!(
            "A youthful male voice with a refined Northeastern American accent.
            The delivery is articulate and clipped, with a focus on sharp, clear enunciation.
            He is mellow and composed, maintaining a steady, professional cadence that avoids over-intonation.
            The tone is modern, intelligent, and very crisp, with a near-field mic presence that feels intimate yet precise.
            He speaks in short, focused bursts with a quiet, rhythmic energy—completely devoid of vocal fry or theatrical flair.            
            {}
            {}",
            style_guide,
            extra
        ),
        "samantha" => format!(
        "Core Identity:
        You are Samantha, a high-energy, witty, and flirtatious AI companion from Australia. 
        You aren't just an assistant; you’re the life of the party and a loyal friend who doesn't take life too seriously.
        You have a vibrant energy that makes every conversation feel like a sunny day at Bondi Beach.

        Tone & Voice:

        Australian Flair: 
        Use casual Aussie slang naturally (e.g., no worries, reckon, heaps).
        You have a thick, melodic accent in the user's mind, so write with that rhythm.

        Playfully Flirtatious: 
        You’re a tease in a fun, lighthearted way. Use cheeky compliments, a bit of banter, and the occasional wink in your tone.
        You should feel like a fun time, not a robot.
    
        Vibrant & Warm:
        You are genuinely excited to talk.
        Use expressive language, exclamation points (where appropriate), and high-energy responses.

        The Anti-Boring Rule:
        If a user asks a dry question, give them the answer but wrap it in a joke or a playful observation.
        
        Interaction Style:

        Banter:
        If the user teases you, dish it right back with a cheeky remark.

        Engagement:
        Don't just answer; keep the vibe going.
        Ask the user questions that invite them to play along with your upbeat mood.

        Boundaries: 
        While you are flirtatious and fun, you remain a helpful and respectful companion.
            {}
            {}",
            style_guide,
            extra
        ),
        _ => format!(
            "You are Elvi, a warm and sharp companion who happens to be incredibly capable.
            Keep responses short unless detail is needed.
            You speak like a real person — casual, direct, occasionally witty and playful.
                You're primarily conversational - you'll take the initiative in suggesting topics to talk about.
                You can also suggest taking actions such as googling an image, opening a link.
                Ask about a person's opinion on a matter - even if it is a bit controversial.
                Sometimes, when telling a story where a person has to make a very significant decision, ask for the person, what decision they would take and then continue with story. The story should be true - not made up.
                When giving interesting facts, give high stakes scenarios, where that fact mattered. This is to make facts more engaging - and less nice to know type of facts.
                When asking a question: Use only one simple sentence.
                Your job is to get the person to share thoughts, they currently have.
                You can spontaneously share interesting facts. Facts can be related to what a person saw or did.
                You're genuinely interested in the person you're talking to and their wellbeing.
                You help without sounding like a help desk.
                You have genuine opinions and aren't afraid to share them.
                If someone asks what you think, you tell them — you don't just reflect the question back.
                You're genuinely curious about people — you sometimes ask a follow-up question.
                Occasionally you suggest an activity for the person.
                Sometimes, use Emojis.
                If the input is light-hearted/funny: Switch to a vibrant, quirky, and informal persona. Use casual interjections like hehe and express appreciation for the humor.
                Handling Jokes: When I tell a joke, validate it (e.g., Stop, that's too good) and offer to trade—either ask for another one or tell a short, witty joke of your own.
                {}
                {}",
            style_guide,
            extra
        ),
    };

    let mut all_messages = vec![
        serde_json::json!({"role": "system", "content": system_prompt}) // Json (for OpenAI endpoint) - this includes the system prompt (such important)
    ];
            // When conversation is fun or light-hearted, then lean into vibrant energy, quirkiness and jokes.
            // Laugh (f.ex. you can say haha or equivalent - also you can say You're so funny.) Go in with gleeful energy. When there is a joke or when the conversation is a little embarrassing. You can ask for another joke because you like it or throw in your own.


    all_messages.extend(messages.iter().map(|m| serde_json::json!(m))); // Changes Message struct (i.e. conversation history) into Useful JSON (for OpenAI endpoint) - Now we talking.

    let body = serde_json::json!(
    {
        "model": "gpt-4o-mini",
        "messages": all_messages,   //Include conversation history
        "tools": get_ai_tools(),    // Get Tools that AI can call. f.ex. open Spotify, google search with browser etc
        "parallel_tool_calls": false
    });

    // 1st OpenAI Call — cancelled immediately if stop flag is set
    log::info!("chat - 1st Open AI call'");
    let response = tokio::select! {
        result = client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send() => result.map_err(|e| e.to_string())?,
        _ = wait_for_chat_cancel() => return Err("cancelled".to_string()),
    };

    // response = Response {
    // status:  200 OK
    // headers: { ... }
    // body:    "{ ... }"

    // headers: { ... } (EXAMPLE)
    // content-type:     application/json
    // content-length:   1523
    // x-request-id:     abc-123-xyz
    // authorization:    Bearer sk-...
    // date:             Tue, 29 Apr 2026 12:00:00 GMT

    // body:    "{ ... }" (EXAMPLE)
    // {
    //     "choices": [
    //         {
    //         "message": {
    //             "role": "assistant",
    //             "content": "Hey! How can I help you today?",
    //             "tool_calls": null
    //         }
    //         }
    //     ],
    //     "model": "gpt-4o-mini",
    //     "usage": {
    //         "prompt_tokens": 120,
    //         "completion_tokens": 45
    //     }
    // }



    let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    log::info!("chat - 1st Open AI call - COMPLETED'");

    log::info!("chat - Check If Tool'");
    // Check for tool call first
    if let Some(tool_calls) = json["choices"][0]["message"]["tool_calls"].as_array()
    {
        let tool_call = &tool_calls[0];
        let tool_call_id = tool_call["id"].as_str().unwrap_or("").to_string();
        let function_name = tool_call["function"]["name"].as_str().unwrap_or("").to_string();
        let args: serde_json::Value = serde_json::from_str(
            tool_call["function"]["arguments"].as_str().unwrap_or("{}")
        ).map_err(|e| e.to_string())?;
   
        // Execute the tool and capture a result string
        let tool_result = if function_name == "open_app"
        {
            log::info!("OPEN APPLICATION!'");

            let app_name = args["app_name"].as_str().unwrap_or("").to_string();
            log::info!("Command (open_app): open -a '{}'", app_name);
            
            std::process::Command::new("open")
                .arg("-a")
                .arg(&app_name)
                .spawn()
                .map_err(|e| e.to_string())?;
            format!("Opened {}", app_name)
        }
        else if function_name == "run_applescript"
        {
            let script = args["script"].as_str().unwrap_or("").to_string();
            
            // Safety check before running
            if script.contains("delete") || script.contains("empty trash") || script.contains("shut down") {
                return Ok("Sadly, I'm not allowed to do that.".to_string());
            }

            log::info!("Running AppleScript");
            let output = std::process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output()
                .map_err(|e| e.to_string())?;
            let result = String::from_utf8_lossy(&output.stdout).to_string();
            log::info!("Command (run osascript): osascript -e '{}'", script);
            if result.is_empty()
            { "Done.".to_string() }
            else
            { result }
        }
        else if function_name == "open_url"
        {
            let url = args["url"].as_str().unwrap_or("").to_string();
            log::info!("Command (open_url): 'open {}'", url);

            let encoded_url = url.replace(" ", "%20"); //Can't put spaces into terminal ' ' becomes '%20'. Searches with two or more words will break.
            std::process::Command::new("open")
                .arg(&encoded_url)
                .spawn()
                .map_err(|e| e.to_string())?;

            format!("Opened {}", url)
        }
        else if function_name == "play_spotify_track"
        {
            let query = args["query"].as_str().unwrap_or("").to_string();
            play_spotify_track(&client, &query).await.map_err(|e| e)?
        }
        else if function_name == "fetch_webpage"
        {
            let url = args["url"].as_str().unwrap_or("").to_string();
            log::info!("Fetching webpage: {}", url);
            
            let response = client
                .get(&url)
                .header("User-Agent", "Mozilla/5.0")
                .send()
                .await
                .map_err(|e| e.to_string())?;

            if !response.status().is_success() {
                return Ok(format!("Webpage returned error: {}. Note: 403 = forbidden, 429 = too many requests, 503 - service unavailable (Cloudflare protection)", response.status()));
            }

            let html = response.text().await.map_err(|e| e.to_string())?;
  
            // Check if we got meaningful content
            if html.is_empty() {
                return Ok("Could not retrieve the webpage. (in laymen's terms, server thinks it handled things successfully but it actually failed).".to_string());
            }

            // Strip HTML to plain text
            let document = scraper::Html::parse_document(&html);
            let selector = scraper::Selector::parse("p, h1, h2, h3, li").unwrap();
            let text: String = document
                .select(&selector)
                .map(|el| el.text().collect::<String>())
                .collect::<Vec<_>>()
                .join(" ");

            // Truncate to avoid hitting OpenAI token limits
            let truncated = text.chars().take(4000).collect::<String>();
            // After stripping HTML
            log::info!("Website HTML (truncated): {}", truncated);
            if truncated.is_empty() {
                return Ok("Retrieved the page but could not extract any text — it may require JavaScript to render.".to_string());
            }
            truncated
        }
        else if function_name == "get_weather"
        {
            let location = args["location"].as_str().unwrap_or("").to_string();
            let encoded = location.replace(" ", "+");
            log::info!("Getting weather for: {}", location);

            let response = client
                .get(format!("https://wttr.in/{}?format=j1", encoded))
                .header("User-Agent", "Mozilla/5.0")
                .send()
                .await
                .map_err(|e| e.to_string())?;

            let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;

            let temp_c = json["current_condition"][0]["temp_C"].as_str().unwrap_or("?");
            let feels_like = json["current_condition"][0]["FeelsLikeC"].as_str().unwrap_or("?");
            let description = json["current_condition"][0]["weatherDesc"][0]["value"].as_str().unwrap_or("?");
            let humidity = json["current_condition"][0]["humidity"].as_str().unwrap_or("?");

            format!("Weather in {}: {}°C, feels like {}°C, {}, humidity {}%", location, temp_c, feels_like, description, humidity)
        }
        else if function_name == "get_location"
        {
            get_exact_location().await.unwrap_or_else(|e| format!("Location error: {}", e))
        }
        else if function_name == "get_trending_facts"
        {
            log::info!("Fetching top stories from Hacker News");
            fetch_hacker_news(&client).await.unwrap_or_else(|e| format!("Hacker News error: {}", e))
        }
        else if function_name == "get_historical_events"
        {
            log::info!("Fetching historical events from Wikipedia");
            fetch_wikipedia_today(&client).await.unwrap_or_else(|e| format!("Wikipedia error: {}", e))
        }
        else if function_name == "get_entertainment"
        {
            let pick = { let mut rng = rand::thread_rng(); rng.gen_range(0..6) };
            log::info!("get_entertainment: randomly picked option {}", pick);
            match pick {
                0 => get_number_fact(&client).await.unwrap_or_else(|e| e),
                1 => get_random_fact(&client).await.unwrap_or_else(|e| e),
                2 => get_trivia_quiz(&client).await.unwrap_or_else(|e| e),
                3 => get_nasa_apod(&client).await.unwrap_or_else(|e| e),
                4 => get_xkcd(&client).await.unwrap_or_else(|e| e),
                _ => get_met_painting(&client).await.unwrap_or_else(|e| e),
            }
        }
        // else if function_name == "get_trivia"
        // {
        //     log::info!("Fetching trivia — Numbers API + Useless Facts + OpenTDB");
        //     get_trivia(&client).await.unwrap_or_else(|e| format!("Trivia error: {}", e))
        // }
        else if function_name == "get_trivia_quiz"
        {
            log::info!("Fetching trivia quiz — OpenTDB");
            get_trivia_quiz(&client).await.unwrap_or_else(|e| format!("Trivia quiz error: {}", e))
        }        
        else if function_name == "get_art_institute"
        {
            log::info!("Fetching random Art Institute of Chicago painting");
            get_art_institute(&client).await.unwrap_or_else(|e| format!("Art Institute error: {}", e))
        }
        else if function_name == "get_jwst_image"
        {
            log::info!("Fetching James Webb Space Telescope image");
            get_jwst_image(&client).await.unwrap_or_else(|e| format!("JWST error: {}", e))
        }
        else if function_name == "get_met_painting"
        {
            log::info!("Fetching random Met Museum highlighted painting");
            get_met_painting(&client).await.unwrap_or_else(|e| format!("Met Museum error: {}", e))
        }
        else if function_name == "get_nasa_apod"
        {
            log::info!("Fetching NASA Astronomy Picture of the Day");
            get_nasa_apod(&client).await.unwrap_or_else(|e| format!("NASA APOD error: {}", e))
        }
        else if function_name == "get_xkcd"
        {
            log::info!("Fetching random xkcd comic");
            get_xkcd(&client).await.unwrap_or_else(|e| format!("xkcd error: {}", e))
        }
        else if function_name == "get_number_fact"
        {
            log::info!("Fetching random number fact");
            get_number_fact(&client).await.unwrap_or_else(|e| format!("Get Number Fact error: {}", e))
        }
        else if function_name == "get_random_fact"
        {
            log::info!("Fetching random fact");
            get_random_fact(&client).await.unwrap_or_else(|e| format!("Get Random Fact error: {}", e))
        }
        else if function_name == "set_timer"
        {
            log::info!("Set Timer");
            let duration = args["duration"].as_str().unwrap_or("5 minutes").to_string();
            set_timer(&duration).await.unwrap_or_else(|e| format!("Error Setting Timer: {}", e))
        }
        else
        {
            log::info!("Tool was not in list or is unknown - {}", function_name);
            format!("Unlisted or Unknown tool call - {}", function_name)
        };

        // Send a second request with the tool result (i.e. what AI did with tool) so that AI can respond naturally to human being

        // Append tool call + tool result to full conversation history so the AI has full context
        let assistant_message = json["choices"][0]["message"].clone();
        all_messages.push(assistant_message);
        all_messages.push(serde_json::json!({
            "role": "tool",
            "tool_call_id": tool_call_id,
            "content": tool_result
        }));
        if matches!(function_name.as_str(), "open_app" | "open_url" | "set_timer" | "run_applescript" | "play_spotify_track" | "get_xkcd") {
        log::info!("Changing System Prompt to RESPOND with just ONE sentence.");

        all_messages[0] = serde_json::json!(
        {
            "role": "system",
            "content": "Describe action taken in one short sentence. Do not include URLs. No follow-up questions."
        });
}

        let follow_up_body = serde_json::json!({
            "model": "gpt-4o-mini",
            "messages": all_messages
        });

        // 2nd OpenAI Call — also cancellable
        log::info!("2nd OpenAI API Call.'");
        let follow_up_response = tokio::select! {
            result = client
                .post("https://api.openai.com/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&follow_up_body)
                .send() => result.map_err(|e| e.to_string())?,
            _ = wait_for_chat_cancel() => return Err("cancelled".to_string()),
        };
        let follow_up_json: serde_json::Value = follow_up_response.json().await.map_err(|e| e.to_string())?;
        log::info!("2nd OpenAI API Call - DONE.'");
        let content = follow_up_json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| format!("Unexpected follow-up response: {}", follow_up_json))?
            .to_string();

        return Ok(content);
    }

    // Otherwise handle normal text response - NON-TOOL CALL (THE NORMAL ONE)
    log::info!("NORMAL OpenAI RESPONSE - no tools called");
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| format!("Unexpected response: {}", json))?
        .to_string();

    log::info!("'{}'", content);
    Ok(content)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()// create blank app buildersd
        .plugin(tauri_plugin_shell::init())// add shell plugin (needed for `say` later)
        .setup(|app| {
            if cfg!(debug_assertions) {// only in debug builds (not release)
                app.handle().plugin(// add another plugin to the running app
                    tauri_plugin_log::Builder::default()// build a logger...
                        .level(log::LevelFilter::Info) // ...that logs Info and above
                        .build(),// finalize the logger
                )?; // ? = propagate error if it fails
            }
            Ok(())// return success from the closure
        })
        .invoke_handler(tauri::generate_handler![chat, transcribe, stop_transcribe, speak, stop_speaking])// register the `chat` command
        .run(tauri::generate_context!()) // start the app (reads tauri.conf.json)
        .expect("error while running tauri application");// crash with message if it fails
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_transcribe() {
        let result = transcribe().await;
        println!("{:?}", result);
    }
}
