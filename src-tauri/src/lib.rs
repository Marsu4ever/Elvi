use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock, atomic::{AtomicBool, Ordering}};
use rand::Rng;

mod transcribe;
use transcribe::{transcribe, stop_transcribe};

mod speak;
use speak::{speak, stop_speaking};

mod system_prompt;
use system_prompt::{get_system_prompt};

mod ai_tools;
use ai_tools::{get_ai_tools};

// Cancels in-flight OpenAI chat requests
static STOP_CHAT: OnceLock<Arc<AtomicBool>> = OnceLock::new();


fn get_stop_chat() -> Arc<AtomicBool> {
    STOP_CHAT.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
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


#[derive(Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

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

    // 4. Open the comic page in the browser automatically
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

async fn execute_tool(function_name: &str, args: &serde_json::Value, client: &reqwest::Client)
    -> Result<String, String> {

    // Execute the tool and capture a result string
    let tool_result =  if function_name == "open_app"
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
    else if function_name == "stop_spotify"
    {
        log::info!("tool_call: stop_spotify");
        std::process::Command::new("osascript")
            .arg("-e")
            .arg("tell application \"Spotify\" to pause")
            .output()
            .map_err(|e| e.to_string())?;
        "Spotify paused".to_string()
    }
    else if function_name == "play_spotify"
    {
        log::info!("tool_call: play_spotify");
        std::process::Command::new("osascript")
            .arg("-e")
            .arg("tell application \"Spotify\" to play")
            .output()
            .map_err(|e| e.to_string())?;
        "Spotify resumed".to_string()
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
    else
    {
        log::info!("Tool was not in list or is unknown - {}", function_name);
        format!("Unlisted or Unknown tool call - {}", function_name)
    };
    Ok(tool_result)
}


async fn run_tool_call(tool_calls: &[serde_json::Value], json: &serde_json::Value,
    messages: Vec<Message>, mut all_messages: Vec<serde_json::Value>,
    client: &reqwest::Client, api_key: &str,) -> Result<String, String>
{
    let tool_call = &tool_calls[0];
    let tool_call_id = tool_call["id"].as_str().unwrap_or("").to_string();
    let function_name = tool_call["function"]["name"].as_str().unwrap_or("").to_string();
    let args: serde_json::Value = serde_json::from_str(
        tool_call["function"]["arguments"].as_str().unwrap_or("{}")
    ).map_err(|e| e.to_string())?;

    let tool_result = execute_tool(&function_name, &args, &client).await?;

    // Send a second request with the tool result (i.e. what AI did with tool) so that AI can respond naturally to human being

    // Append tool call + tool result to full conversation history so the AI has full context
    let assistant_message = json["choices"][0]["message"].clone();
    all_messages.push(assistant_message.clone());
    all_messages.push(serde_json::json!({
        "role": "tool",
        "tool_call_id": tool_call_id,
        "content": tool_result
    }));

    let follow_up_messages = if matches!(function_name.as_str(), "open_app" | "open_url" | "run_applescript" | "play_spotify_track" | "get_xkcd") {
        // Minimal context — just confirm what was done (Otherwise AI is i.verbose (when not needed) and ii. confuses other parts of chat to response (f.ex. thinks it can't do tool calls))
        vec![
            serde_json::json!({
                "role": "system",
                "content": "Describe action taken in one short sentence. Do not include URLs. No follow-up questions."
            }),
            serde_json::json!({
                "role": "user",
                "content": messages.last().map(|m| m.content.as_str()).unwrap_or("")
            }),
            assistant_message,
            serde_json::json!({
                "role": "tool",
                "tool_call_id": tool_call_id,
                "content": tool_result
            })
        ]
    } else {
        // Full context — all messages with tool call appended
        all_messages
    };

    let follow_up_body = serde_json::json!({
        "model": "gpt-4o-mini",
        "messages": follow_up_messages
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

#[tauri::command]
async fn chat(messages: Vec<Message>, bot: String) -> Result<String, String>
{
    // Reset only the chat flag — audio flag is managed separately by speak()
    get_stop_chat().store(false, Ordering::Relaxed);

    if let Some(last) = messages.last() {
        log::info!("'{}'", last.content);
    }
    log::info!("chat function - START'");

    // OpenAI API KEY
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| "OPENAI_API_KEY environment variable not set".to_string())?;

    log::info!("chat - reqwest call'");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;


    let system_prompt = get_system_prompt(&bot); // Get system prompt = bot personailty (f.ex. Elvi)

    let mut all_messages = vec![
        serde_json::json!({"role": "system", "content": system_prompt}) // Json (for OpenAI endpoint) - this includes the system prompt (such important)
    ];

    all_messages.extend(messages.iter().map(|m| serde_json::json!(m))); // Changes Message struct (i.e. conversation history) into Useful JSON (for OpenAI endpoint) - Now we talking.

    // OpenAI Verson
       let body = serde_json::json!(
    {
        "model": "gpt-4-turbo", // gpt-4o [best balance of speed and intelligence], gpt-4o-mini [what you have now, fast and cheap], o3-mini [strong reasoning, good for complex questions], claude-sonnet-4-5 [very strong, great conversation quality], claude-haiku-3-5 [fast and cheap, similar tier to gpt-4o-mini], grok-3, grok-3-mini
        "messages": all_messages,   //Include conversation history
        "tools": get_ai_tools(),    // Get Tools that AI can call. f.ex. open Spotify, google search with browser etc
        "parallel_tool_calls": false
    });

    // OpenAI
    let api_url = "https://api.openai.com/v1/chat/completions";

    // 1st OpenAI Call — cancelled immediately if stop flag is set
    log::info!("Chat - 1st Open AI call'");
    let response = tokio::select! {
        result = client
            .post(api_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send() => result.map_err(|e| e.to_string())?,
        _ = wait_for_chat_cancel() => return Err("cancelled".to_string()),
    };

    // STRUCTURE OF OPENAI RESPONSES - below

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

    log::info!("chat - Check If Tool'");
    // Check for tool call first
    if let Some(tool_calls) = json["choices"][0]["message"]["tool_calls"].as_array()
    {
        // Run Tool call + 2nd API call for response to user
        return run_tool_call(tool_calls, &json, messages, all_messages, &client, &api_key,).await;   
    }

    // Otherwise handle normal text response - (THE NORMAL ONE) (i.e. non-tool call)
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
    tauri::Builder::default()                   // create blank app buildersd
        .plugin(tauri_plugin_shell::init())     // add shell plugin (needed for `say` later)
        .setup(|app| {
            if cfg!(debug_assertions) {         // only in debug builds (not release)
                app.handle().plugin(            // add another plugin to the running app
                    tauri_plugin_log::Builder::default()// builds a logger
                        .level(log::LevelFilter::Info) // logs info
                        .build(),               // finalize the logger
                )?;                             // propagate error if it fails
            }
            Ok(())                              // return success from the closure
        })
        .invoke_handler(tauri::generate_handler![chat, transcribe, stop_transcribe, speak, stop_speaking])// register commands (f.ex. chat)
        .run(tauri::generate_context!())        // start the app (reads tauri.conf.json)
        .expect("error while running tauri application");   // crash with message if it fails
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
