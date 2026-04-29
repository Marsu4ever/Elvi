import { invoke } from "@tauri-apps/api/core";

interface Message {
  role: "user" | "assistant";
  content: string;
}

const botThemes: Record<string, { accent: string; accentLight: string; accent2: string; accentRgb: string; glowBg: string; dot1: string; dot2: string; dot3: string; gradMid: string; gradDeep: string; gradDeepest: string }> = {
  elvi:     { accent: "#7c6af7", accentLight: "#a78bfa", accent2: "#5eb8f5", accentRgb: "124, 106, 247", glowBg: "#4a1fa8", dot1: "#e0b0ff", dot2: "#5eb8f5", dot3: "#f59b5e", gradMid: "#e8d5ff", gradDeep: "#2d0a6e", gradDeepest: "#1a0050" },
  alex:     { accent: "#0d9488", accentLight: "#2dd4bf", accent2: "#0e7490", accentRgb: "13, 148, 136",  glowBg: "#0d9488", dot1: "#a7f3d0", dot2: "#06b6d4", dot3: "#fbbf24", gradMid: "#d1faf5", gradDeep: "#064e3b", gradDeepest: "#022c22" },
  samantha: { accent: "#f472b6", accentLight: "#f9a8d4", accent2: "#c084fc", accentRgb: "244, 114, 182", glowBg: "#db2777", dot1: "#fda4af", dot2: "#c084fc", dot3: "#818cf8", gradMid: "#fce7f3", gradDeep: "#831843", gradDeepest: "#500724" },
};

function makeOrbSvg(bot: string, size: number): string {
  const t = botThemes[bot] ?? botThemes.elvi;
  const id = `hdr-grad-${bot}`;
  return `<svg width="${size}" height="${size}" viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg">
    <defs>
      <radialGradient id="${id}" cx="50%" cy="50%" r="50%">
        <stop offset="0%"   stop-color="#ffffff" stop-opacity="1"/>
        <stop offset="20%"  stop-color="${t.gradMid}"/>
        <stop offset="50%"  stop-color="${t.accent}"/>
        <stop offset="80%"  stop-color="${t.gradDeep}" stop-opacity="0.85"/>
        <stop offset="100%" stop-color="${t.gradDeepest}" stop-opacity="0"/>
      </radialGradient>
    </defs>
    <circle cx="16" cy="16" r="14" fill="${t.glowBg}" opacity="0.18"/>
    <circle cx="16" cy="16" r="14" stroke="${t.accentLight}" stroke-width="0.6" stroke-dasharray="5 8" opacity="0.4"/>
    <circle cx="16" cy="16" r="11.5" stroke="${t.accent2}" stroke-width="0.5" stroke-dasharray="2 5" opacity="0.35"/>
    <circle cx="16" cy="16" r="10.5" fill="url(#${id})"/>
    <circle cx="16" cy="16" r="4" fill="white" opacity="0.9"/>
    <circle cx="16" cy="2.5" r="1.3" fill="${t.dot1}">
      <animate attributeName="opacity" values="0.9;0.4;0.9" dur="4s" repeatCount="indefinite"/>
    </circle>
    <circle cx="27.3" cy="22.5" r="1" fill="${t.dot2}">
      <animate attributeName="opacity" values="0.5;1;0.5" dur="4s" repeatCount="indefinite"/>
    </circle>
    <circle cx="4.7" cy="22.5" r="1" fill="${t.dot3}">
      <animate attributeName="opacity" values="0.7;0.2;0.7" dur="3.5s" repeatCount="indefinite"/>
    </circle>
  </svg>`;
}

function applyTheme(bot: string) {
  const t = botThemes[bot] ?? botThemes.elvi;
  const r = document.documentElement.style;
  r.setProperty("--accent",       t.accent);
  r.setProperty("--accent-light", t.accentLight);
  r.setProperty("--accent-2",     t.accent2);
  r.setProperty("--accent-rgb",   t.accentRgb);
  const avatars: Record<string, string> = {
    elvi:     "./avatars/elvi.svg",
    alex:     "./avatars/alex.svg",
    samantha: "./avatars/samantha.svg",
  };

  if (avatars[bot]) {
    headerOrbWrap.innerHTML = `<img src="${avatars[bot]}" alt="${bot}" style="height:64px;width:auto;object-fit:contain;filter:drop-shadow(0 0 6px rgba(${t.accentRgb},0.7));">`;
  } else {
    headerOrbWrap.innerHTML = makeOrbSvg(bot, 34);
  }
}

const histories: Record<string, Message[]> = { elvi: [], alex: [], samantha: [] };
let selectedBot = "elvi";
let messages: Message[] = histories[selectedBot];

const landingEl = document.getElementById("landing")!;
const appEl = document.getElementById("app")!;
const headerNameEl = document.getElementById("header-name")!;
const headerOrbWrap = document.getElementById("header-orb-wrap")!;
const backBtn = document.getElementById("back-btn")!;
const messagesEl = document.getElementById("messages")!;
const inputEl = document.getElementById("user-input") as HTMLTextAreaElement;
const sendBtn = document.getElementById("send-btn")!;
const micBtn = document.getElementById("mic-btn")!;
const voiceToggleBtn = document.getElementById("voice-toggle")!;
const voiceOnIcon = document.getElementById("voice-on-icon")!;
const voiceOffIcon = document.getElementById("voice-off-icon")!;

// Dock magnification effect
const botList = document.getElementById("bot-list")!;
const botOptions = Array.from(document.querySelectorAll<HTMLElement>(".bot-option"));

const MAX_SCALE = 1.55;
const MIN_SCALE = 0.82;
const INFLUENCE = 70;
const LERP = 0.07; // lower = slower follow (try 0.04–0.15)

const currentScales = botOptions.map(() => 1);
const targetScales  = botOptions.map(() => 1);
let rafId: number | null = null;

function animateScales() {
  let settled = true;
  botOptions.forEach((el, i) => {
    const diff = targetScales[i] - currentScales[i];
    if (Math.abs(diff) > 0.001) {
      currentScales[i] += diff * LERP;
      settled = false;
    } else {
      currentScales[i] = targetScales[i];
    }
    el.style.transform = `scale(${currentScales[i]})`;
  });
  rafId = settled ? null : requestAnimationFrame(animateScales);
}

botList.addEventListener("mousemove", (e) => {
  botOptions.forEach((el, i) => {
    const rect = el.getBoundingClientRect();
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    const dist = Math.sqrt((e.clientX - cx) ** 2 + (e.clientY - cy) ** 2);
    const t = Math.max(0, 1 - dist / INFLUENCE);
    targetScales[i] = MIN_SCALE + (MAX_SCALE - MIN_SCALE) * t;
  });
  if (!rafId) rafId = requestAnimationFrame(animateScales);
});

botList.addEventListener("mouseleave", () => {
  botOptions.forEach((_, i) => { targetScales[i] = 1; });
  if (!rafId) rafId = requestAnimationFrame(animateScales);
});

// Back button
backBtn.addEventListener("click", () => {
  invoke("stop_speaking");
  setSpeaking(false);
  applyTheme("elvi");
  appEl.style.display = "none";
  landingEl.style.display = "flex";
  landingEl.style.animation = "none";
  requestAnimationFrame(() => { landingEl.style.animation = ""; });
});

// Bot selection
document.querySelectorAll(".bot-option").forEach((el) => {
  el.addEventListener("click", () => {
    selectedBot = (el as HTMLElement).dataset.bot ?? "elvi";
    messages = histories[selectedBot];
    applyTheme(selectedBot);
    messagesEl.innerHTML = "";
    messages.forEach(m => appendMessage(m.role as "user" | "assistant", m.content));
    const name = el.querySelector(".bot-name")!.textContent ?? "Elvi";
    headerNameEl.textContent = name;
    landingEl.style.display = "none";
    appEl.style.display = "flex";
    appEl.style.animation = "none";
    requestAnimationFrame(() => { appEl.style.animation = ""; });
    inputEl.focus();
  });
});

let voiceEnabled = true;

voiceToggleBtn.addEventListener("click", () => {
  voiceEnabled = !voiceEnabled;
  voiceToggleBtn.classList.toggle("muted", !voiceEnabled);
  voiceOnIcon.style.display = voiceEnabled ? "" : "none";
  voiceOffIcon.style.display = voiceEnabled ? "none" : "";
  if (!voiceEnabled) {
    invoke("stop_speaking");
  }
});;

function appendMessage(role: "user" | "assistant", content: string) {
  const row = document.createElement("div");
  row.classList.add("message-row", role);

  const bubble = document.createElement("div");
  bubble.classList.add("message");

  const linkRegex = /\[link:(https?:\/\/[^\]]+)\]/g;
  if (linkRegex.test(content)) {
    const parts = content.split(/\[link:(https?:\/\/[^\]]+)\]/g);
    const matches = [...content.matchAll(/\[link:(https?:\/\/[^\]]+)\]/g)];
    parts.forEach((part, i) => {
      if (part) bubble.appendChild(document.createTextNode(part));
      if (matches[i]) {
        const a = document.createElement("a");
        a.href = matches[i][1];
        a.textContent = "View image →";
        a.target = "_blank";
        a.style.display = "block";
        a.style.marginTop = "6px";
        bubble.appendChild(a);
      }
    });
  } else {
    bubble.textContent = content;
  }

  row.appendChild(bubble);
  messagesEl.appendChild(row);
  messagesEl.scrollTop = messagesEl.scrollHeight;
}

const stopIcon = `<svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor"><rect x="2" y="2" width="10" height="10" rx="1.5"/></svg>`;
const sendIcon = `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="22" y1="2" x2="11" y2="13"></line><polygon points="22 2 15 22 11 13 2 9 22 2"></polygon></svg>`;

let isSpeaking = false;
let generation = 0;

function setSpeaking(speaking: boolean) {
  isSpeaking = speaking;
  sendBtn.innerHTML = speaking ? stopIcon : sendIcon;
}

sendBtn.addEventListener("click", () => {
  const hasText = inputEl.value.trim().length > 0;
  if (hasText) {
    sendMessage();
  } else if (isSpeaking) {
    invoke("stop_speaking");
    setSpeaking(false);
  }
});

function appendThinking(): HTMLElement {
  const row = document.createElement("div");
  row.classList.add("message-row", "assistant", "thinking-row");
  const bubble = document.createElement("div");
  bubble.classList.add("message", "thinking-bubble");
  bubble.innerHTML = `<span class="dot"></span><span class="dot"></span><span class="dot"></span>`;
  row.appendChild(bubble);
  messagesEl.appendChild(row);
  messagesEl.scrollTop = messagesEl.scrollHeight;
  return row;
}

async function sendMessage() {
  const content = inputEl.value.trim();
  if (!content) return;

  if (isSpeaking) {
    invoke("stop_speaking");
  }

  const myGen = ++generation;

  inputEl.value = "";
  inputEl.style.height = "auto";

  messages.push({ role: "user", content });
  appendMessage("user", content);
  setSpeaking(true);

  const thinkingRow = appendThinking();

  try {
    const reply = await invoke<string>("chat", { messages, bot: selectedBot });
    thinkingRow.remove();
    if (myGen !== generation) return;

    messages.push({ role: "assistant", content: reply });
    appendMessage("assistant", reply);
    if (voiceEnabled) {
      await invoke("speak", { text: reply , bot: selectedBot});
    }
  } catch (err) {
    thinkingRow.remove();
    if (myGen !== generation) return;
    if (String(err) !== "cancelled") {
      appendMessage("assistant", `Error: ${err}`);
    }
  } finally {
    if (myGen === generation) {
      setSpeaking(false);
      inputEl.focus();
    }
  }
}

inputEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    sendMessage();
  }
});

let isRecording = false;

micBtn.addEventListener("click", async () => {
  if (isRecording) {
    invoke("stop_transcribe");
    return;
  }

  invoke("stop_speaking");
  setSpeaking(false);
  isRecording = true;
  micBtn.classList.add("recording");
  inputEl.placeholder = "Recording... (click to stop)";

  try {
    const transcript = await invoke<string>("transcribe");
    inputEl.value = transcript;
    inputEl.style.height = "auto";
    inputEl.style.height = inputEl.scrollHeight + "px";
    inputEl.focus();
  } catch (err) {
    // silently ignore cancellation
  } finally {
    isRecording = false;
    micBtn.classList.remove("recording");
    inputEl.placeholder = "Say something...";
  }
});

inputEl.addEventListener("input", () => {
  inputEl.style.height = "auto";
  inputEl.style.height = inputEl.scrollHeight + "px";
});
