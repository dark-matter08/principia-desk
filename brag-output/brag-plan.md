# Brag Plan: Principia Desk

## What is this app?
A free, open-source desktop study desk (Tauri + Svelte, macOS/Windows/Linux) that schedules classes as durable appointments, has your own AI tutor write each lesson from primary sources *before* the bell rings, and then holds the screen — with a focus policy you choose per class and five documented ways out of every lock.

## The angle
"The lesson is written before the bell." Nothing about this app waits for you to feel like it: a study time is an appointment, the desk prepares for it, rings, holds the door, and lets go when the check is done. The video is built in the app's own idiom — "the UI is the diagram": blueprint grid, mono meta-labels, status LEDs, node panels, one Fraunces headline per screen. The tension that makes it specific: a desk strict enough to say `HARD · no mercy` and honest enough to promise five ways out.

## Hook (first 2-3 seconds)
The countdown ring from the Today page, alone on the blueprint grid. `NEXT CLASS` in mono, display digits `00:00:03 → 02 → 01 → 00:00:00`, the amber arc closing. On zero the LED flips to teal and a notification slides in: **System Design · 14:58 · lesson ready**. Narration under it: *"Nothing here waits for you to feel like it."* The bell is the hook.

## Key moments (the middle)
- The real Today page rising into frame under the one-line promise, "A study desk that keeps its appointments."
- The real lesson (CAP in practice) with the stage rail lighting one by one: `01 LEARN ~14 min · 02 PRACTICE 10 min · 03 CHECK 6 min · 04 FEEDBACK` — the lesson sized to the session.
- The enforcement tiles arriving one at a time — `ADVISORY · honor system`, `FIRM · level 1000 · escapable`, `HARD · no mercy` — then the keycaps `CONTROL + OPTION + SHIFT + U` and the promise "every lock has five ways out."

## Outro / punchline
Logo mark (the shard ring) settles, "Principia Desk" in Fraunces, "Understand deeply. Practice daily." under it, then a row of mono badges from the site's hero facts: `MIT licence · your keys stay on your machine · nine classes, and your own · every lock has five ways out`, and the URL `principia.ndelucien.com`. Narration: *"Free, open source, and on your machine. Principia Desk."*

## User flow worth showing
1. **Entry** — the appointment comes due: countdown reaches zero, the desk rings, the lesson is already prepared (Today page).
2. **Key action** — the lesson: read → practise → check on the stage rail, the real CAP lesson on screen.
3. **Result / release** — the enforcement level the learner chose, and the desk letting go (the ways out).

## Tone
- Preset: polished
- Creative direction: a quiet premium product film shot inside the app's own blueprint idiom — the UI is the diagram
- Interpretation: few scenes, long settled holds, one serif headline per scene, mono meta-labels for everything else; motion is precise (slides, arcs, LEDs) not bouncy; the narration carries the story and the on-screen text stays short.

## Format: landscape — 1920x1080
## Duration: ~22 seconds (scene lengths flex to the narration; target 21–24s)

## Visual identity (from the project)
- Background: `#141118` (noir), grid dot `#221c29` on a 22px radial-gradient blueprint grid
- Surface / node: `#221d29` / `#1b1721`, border `#3a3344`, divider `#2b2434`
- Accent: `#ef9f27` (amber), accent-fg `#412402`
- Violet: `#534ab7` (fg `#cecbf6`, bg `#26215c`)
- LEDs: ok `#9fe1cb`, warn `#fac775`, err `#f09595`
- Text: `#f2ede4`; muted `#a89f92`; faint `#5e5750`
- Display font: Fraunces (bundled woff2 in `static/fonts/`)
- Body font: Inter (bundled); Mono: JetBrains Mono (bundled)
- Strongest visual element: the amber countdown ring on Today; the numbered stage rail; the ClusterBar strip `principia://today · cluster: local · region: home-1` / `● all systems nominal`; the keycaps in Settings › Recovery; the logo shard ring.
- Screenshots to use: `landing/public/shots/today.png`, `landing/public/shots/lesson.png` (1440x900); recreate the enforcement tiles and keycaps in HTML from `docs/screenshots/17-class-settings-enforcement.png` and `landing/public/shots/recovery.png`.

## Share copy (draft)
Introducing Principia Desk: a study desk that keeps its appointments. Your own AI tutor writes the lesson before the bell, from primary sources; you choose how hard the desk holds you, and every lock has five ways out. Free, open source, macOS/Windows/Linux.

## Audio direction
- Role: warm, restrained bed under narration
- Music: `happy-beats-business-moves-vol-12-by-ende-dot-app.mp3` (steady and clean, ~110 BPM) — the polished pick
- Music treatment: starts at 0 at ~0.30, ducks to ~0.13 while each narration line plays, 1.2s fade-out under the final logo
- Music cue guidance: preset read (`assets/music/cues/happy-beats-business-moves-vol-12-by-ende-dot-app.music-cues.md`, tempo 109.96). Strong cues to target: 8.74s (Today page lands), 13.11s (lesson headline), 17.47s / 18.56s (keycaps / "five ways out"), 22.93s (logo lands). Beat grid for sequential reveals: stage chips near 12.55 · 13.11 · 13.64 · 14.20; enforcement tiles near 15.84 · 16.38 · 16.93 (text reveals hold ≥0.8s settled after the last arrives). Cues bias timing; narration and readability win.
- Audio-reactive treatment: subtle; music RMS/bass may breathe the amber ring glow and the grid vignette. No waveform/equalizer visuals.
- SFX posture: sparse, professional; low HF-risk files; volumes 0.45–0.65
- Audio-coupled moments: countdown ticks (soft, three), the bell at zero, the notification slide, one soft drop per stage chip, a card place per enforcement tile, a soft bong on the logo
- Restraint rule: nothing fights the voice; no stacked hits; no clicks on every element; the music never rises above 0.32.

## Voiceover script
Voice: Kokoro `af_heart`, speed 0.95. One WAV per line so each scene can size itself to its line.

1. Nothing here waits for you to feel like it.
2. Principia Desk. A study desk that keeps its appointments.
3. Before it rings, your own tutor has written the lesson, from primary sources.
4. You choose how hard the desk holds you. And every lock has five ways out.
5. Free, open source, and on your machine. Principia Desk.

## Storyboard

### Scene 1 — The bell — ~3.6s
Blueprint grid. ClusterBar at the top: `principia://today · cluster: local · region: home-1` left, `● all systems nominal` right (teal LED). Centered: the countdown ring — `NEXT CLASS` mono label, display digits in JetBrains Mono counting `00:00:03 → 00:00:02 → 00:00:01 → 00:00:00` one per second, amber arc closing to full. On zero: the LED pulses teal, the ring flashes, and a node-style notification slides in below the ring: `System Design · today · 14:58` / `lesson ready`. The narration is the only prose.
Sequential/interaction: yes — three digit ticks, then the bell + notification arriving.
Audio intent: quiet anticipation, then the bell as a small event, not a slam.
Audio-coupled idea: a soft tick per digit change; a bell hit on zero; a soft drop as the notification lands.
Music: bed fades in under the ticks.
Transition mood: soft → Scene 2 (the ring shrinks toward its place on the Today page as the page rises)

### Scene 2 — Today — ~4.4s
The real Today page (`today.png`) rises into frame as a framed window (slight scale from 0.96 → 1.0, y 40 → 0), landing on the strong cue near 8.7s, with a slow drift toward the countdown ring. Bottom-left overlay on a translucent node panel: kicker mono `FREE · OPEN SOURCE · MACOS, WINDOWS, LINUX`, Fraunces headline **A study desk that keeps its appointments.** Holds settled ≥2s.
Sequential/interaction: none beyond the rise and the headline arriving.
Audio intent: the reveal — confident, calm.
Audio-coupled idea: a soft impact as the page lands; nothing on the headline.
Music: bed at duck level under the line.
Transition mood: soft crossfade → Scene 3

### Scene 3 — The lesson — ~5.0s
The real lesson (`lesson.png`) punched in on its header: `System Design · 30 MIN`, **CAP in practice: choosing partition behaviour, not a permanent label**, the stage rail. Over the rail, four HTML chips light one by one — `01 LEARN ~14 min` → `02 PRACTICE 10 min` → `03 CHECK 6 min` → `04 FEEDBACK` — amber active, muted upcoming, spaced ~0.55s (every beat) then held ≥1.2s. Fraunces caption bottom-left: **The lesson is written before the bell.** A slow pan down the reading (Ken Burns) keeps the page alive.
Sequential/interaction: yes — the four stage chips arriving in order.
Audio intent: steady, working; the tutor has done its part.
Audio-coupled idea: one soft drop per chip (low volume), nothing else.
Music: bed at duck level under the line.
Transition mood: soft → Scene 4

### Scene 4 — Enforcement — ~5.6s
Blueprint grid with a node panel titled `enforcement-service` (mono, violet accent). Three tiles arrive one by one, card-like: `ADVISORY / honor system` (teal), `FIRM / level 1000 · escapable` (amber, active), `HARD / no mercy` (red). Hold ≥0.8s after the last. Then, on "And every lock…", the tiles ease up and a second row arrives: keycaps `CONTROL` + `OPTION` + `SHIFT` + `U` with the mono note `opens the recovery console above every window`, and the Fraunces headline **Every lock has five ways out.** Small mono footnote: `principia-unlock · break glass · three hours`.
Sequential/interaction: yes — three tiles in order, then the keycaps row.
Audio intent: weight for the tiles, then relief for the ways out.
Audio-coupled idea: a card place per tile; a light plate/glass accent as the keycaps land.
Music: bed at duck level; a strong cue near 17.5–18.6s for the keycaps.
Transition mood: soft → Scene 5

### Scene 5 — Outro — ~3.8s
The logo mark (`docs/logo-mark.svg`) settles at center (its LED lights), **Principia Desk** in Fraunces, `Understand deeply. Practice daily.` beneath in Inter. Then a row of mono badges: `MIT licence` · `your keys stay on your machine` · `nine classes, and your own` · `every lock has five ways out`, and `principia.ndelucien.com`. Music fades out under the last second.
Sequential/interaction: yes — logo, name, tagline, then the badge row as one group.
Audio intent: a quiet landing.
Audio-coupled idea: a soft bong as the mark settles; nothing after.
Music: fades out over the final 1.2s.
Transition mood: hold to black.

**Music mood for this video:** polished / steady
**Audio summary:** A calm bed fades in under three countdown ticks and a small bell, ducks under each narration line while sparse, low-risk SFX mark the page landing, the stage chips, the enforcement tiles and the keycaps, and fades out under a single soft bong on the logo.
