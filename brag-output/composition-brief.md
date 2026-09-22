# Hyperframes Composition Brief: Principia Desk

## Objective
Create a short launch-style brag video for Principia Desk, narrated (Kokoro `af_heart`), in the app's own "the UI is the diagram" idiom.

## Output
- Composition directory: `brag-output/composition/`
- Rendered video: `brag-output/brag.mp4`
- Format: landscape — 1920x1080
- Duration: 24.0 seconds (scene lengths flex to the five narration WAVs; total stays within 15–25s)

## Source Material
- Project root: `/Users/darkmatter/Documents/Projects/Personal/system-design-roulette`
- Primary files read: `README.md`, `DESIGN.md`, `landing/src/content/site.json`, `src/lib/theme.css`, `src/lib/fonts.css`, `src/lib/components/EnforcementPicker.svelte`, `src/lib/components/ClusterBar.svelte`, `docs/logo-mark.svg`, screenshots under `landing/public/shots/` and `docs/screenshots/`
- Product name: Principia Desk
- Tagline / strongest claim: "A study desk that keeps its appointments." / "every lock has five ways out"
- Key UI or visual moment to recreate: the amber countdown ring (`NEXT CLASS 00:00:03`) ringing to zero; the stage rail `01 LEARN · 02 PRACTICE · 03 CHECK · 04 FEEDBACK`; the enforcement tiles `ADVISORY honor system / FIRM level 1000 · escapable / HARD no mercy`; the recovery keycaps `CONTROL + OPTION + SHIFT + U`
- Copy that must appear verbatim:
  - `principia://today · cluster: local · region: home-1` and `all systems nominal` (ClusterBar)
  - `NEXT CLASS`
  - `A study desk that keeps its appointments.`
  - `The lesson is written before the bell.`
  - `ADVISORY` / `honor system`, `FIRM` / `level 1000 · escapable`, `HARD` / `no mercy`
  - `Every lock has five ways out.`
  - `Understand deeply. Practice daily.`
  - `MIT licence`, `your keys stay on your machine`, `nine classes, and your own`, `every lock has five ways out`, `principia.ndelucien.com`

## Creative Direction
- Tone preset: polished
- Creative direction: a quiet premium product film shot inside the app's own blueprint idiom — the UI is the diagram
- Interpretation: five scenes, long settled holds, one Fraunces headline per scene, mono meta-labels for everything else; precise motion (slides, arcs, LEDs, dashed pipes), no bounce; narration carries the story, on-screen text stays short.
- Angle: "The lesson is written before the bell." A desk that prepares, rings, holds the door, and lets go — strict enough to say `HARD · no mercy`, honest enough to promise five ways out.
- Hook: the countdown ring alone on the grid, `00:00:03 → 02 → 01 → 00`, then the bell and a `lesson ready` notification. Narration: "Nothing here waits for you to feel like it."
- Outro / punchline: logo mark, "Principia Desk", "Understand deeply. Practice daily.", the four hero-fact badges, the URL. Narration: "Free, open source, and on your machine. Principia Desk."
- Avoid:
  - Generic SaaS language
  - Abstract filler visuals
  - Unrelated visual redesign (no rounded-glass "AI" look; stay noir + amber + violet + teal)

## Visual Identity
- Background: `#141118` with the 22px blueprint dot grid (`#221c29`)
- Text: `#f2ede4`; muted `#a89f92`
- Accent: `#ef9f27` amber; violet `#534ab7` / `#cecbf6`; LED ok `#9fe1cb`, warn `#fac775`, err `#f09595`
- Node surfaces: `#1b1721` on border `#3a3344`, divider `#2b2434`
- Display font: Fraunces (variable, `assets/fonts/fraunces-latin-standard-normal.woff2`)
- Body font: Inter (variable, `assets/fonts/inter-latin-wght-normal.woff2`); Mono: JetBrains Mono (variable, `assets/fonts/jetbrains-mono-latin-wght-normal.woff2`)
- Visual references from the project: `assets/img/today.png`, `assets/img/lesson.png` (real screens), `assets/img/logo-mark.svg`; ClusterBar strip; NodeCard headers; MetaBadge mono pills; numbered rail; keycaps from Settings › Recovery

## Storyboard
Use the storyboard in `brag-output/brag-plan.md` as the creative contract.

Scene summary (global times, beat-aware; the vol-12 grid is ~0.55s):
1. The bell — 0.0–4.4s — ring + `NEXT CLASS`, ticks at 1.09 / 2.19 / 3.27 (beats), bell on zero, notification `System Design · today · 14:58 · lesson ready`
2. Today — 4.1–8.74s — real Today page rises and lands ~4.9; kicker + headline "A study desk that keeps its appointments."
3. The lesson — 8.5–14.2s — real lesson page punched in at 8.74 (strong cue), stage chips at 9.83 / 10.37 / 10.93 / 11.46 (beat-grid), caption at 12.02
4. Enforcement — 13.95–19.8s — tiles at 14.73 / 15.29 / 15.84 (beat-grid), keycaps at 17.47 (strong cue), headline "Every lock has five ways out."
5. Outro — 19.4–24.0s — logo settles ~20.19, name, tagline, badge row, URL; music out by 24.0

## Audio
- Audio role: warm, restrained bed under narration
- Audio arc: bed fades in under the countdown ticks and a small bell; ducks under each of five narration lines; sparse low-HF SFX mark the page landing, the chips, the tiles and the keycaps; a single soft bong on the logo; bed fades out under the last second.
- Music: `assets/music/happy-beats-business-moves-vol-12-by-ende-dot-app.mp3`
- Music treatment: 0 → 0.30 over 0.8s at start; duck to 0.13 (0.3s ramp) 0.3s before each narration line and return to 0.30 (0.5s ramp) after it; fade to 0 over the final 1.4s
- Music cue guidance: bundled preset `assets/music/cues/happy-beats-business-moves-vol-12-by-ende-dot-app.music-cues.json` (109.96 BPM). Strong-cue locks: 8.74 (lesson punch-in), 17.47 (keycaps). Beat-grid: chips 9.83/10.37/10.93/11.46; tiles 14.73/15.29/15.84; countdown ticks 1.09/2.19/3.27.
- Audio-reactive treatment: subtle; `assets/music/audio-data.js` (8 bands @30fps, first 25s) drives the amber ring glow (scene 1) and a full-bleed warmth layer's opacity across the video, via per-frame timeline sets of CSS custom properties. No waveform/equalizer visuals.
- Voiceover: five Kokoro lines in `assets/vo/vo-1..5.wav` (2.33 / 3.39 / 4.35 / 4.20 / 3.61s) on their own track, volume 1.0, starting at 0.35 / 4.7 / 9.0 / 14.4 / 19.9.
- Audio-coupled moments:
  - Scene 1 — three countdown ticks (`interface/drop_001`), bell on zero (`impact/impactBell_heavy_000` at 0.45), notification landing (`interface/drop_002`)
  - Scene 2 — page landing (`impact/impactSoft_medium_001`)
  - Scene 3 — one soft drop per chip (`interface/drop_001` at 0.35)
  - Scene 4 — a card slide per tile (`casino/card-slide-1` at 0.4), keycaps landing (`impact/impactPlate_light_001` at 0.45)
  - Scene 5 — logo settle (`interface/bong_001` at 0.5)
- SFX selection guidance: low-HF-risk families for repeated moments, one medium-risk hit for the bell and the keycaps; nothing over 0.65; nothing stacked.
- SFX analysis guidance: `~/.claude/plugins/cache/brag/brag/0.2.2/skills/brag/assets/sfx/sfx-analysis.md` (read; picks above follow its safe list)
- Exact SFX choice: chosen against the implemented animation; timestamps in `index.html`.
- Audio files: already copied under `brag-output/composition/assets/`

## Hyperframes Instructions
Loaded: `hyperframes-core`, `hyperframes-animation`, `hyperframes-creative` (video-composition, audio-reactive), `hyperframes-keyframes`, `hyperframes-cli`. /brag is its own workflow; no intent interview.

Requirements:
- Show at least one real UI element from the project (two real screens + three recreated components).
- Keep all text readable (mono labels ≥ 20px, headlines 72–96px, ≥0.8s settled holds; sentences ≥0.3s/word).
- 24.0s total.
- Music, SFX and narration as above; music ducks under the voice.
- Cue metadata biases timing; readability and the story win.
- Local assets only (fonts, GSAP vendored at `vendor/gsap.min.js`, audio, images).
- `npx hyperframes check` must pass before render.
