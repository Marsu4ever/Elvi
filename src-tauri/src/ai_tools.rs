pub fn get_ai_tools() -> serde_json::Value
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
            }
        ]
    )
}
