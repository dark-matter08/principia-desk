<script lang="ts">
  /**
   * The lesson as a conversation you hear: a dock at the foot of the
   * reading with play, pause, a line at a time, the speed, and the
   * transcript with the line being spoken lit. Rendered segments play from
   * their files; when the class chose the system voice, or a render failed,
   * the desk reads the lines itself with the voices the class named.
   */
  import { onMount } from 'svelte';
  import { Play, Pause, SkipBack, SkipForward, X, Loader, AudioLines, ListMusic, PenLine } from 'lucide-svelte';
  import { api, onEvent, type LessonAudio } from '$lib/ipc';
  import { assetUrl } from './asset';

  let { sessionId, onclose }: { sessionId: string; onclose: () => void } = $props();

  let audio = $state<LessonAudio | null | undefined>(undefined);
  let error = $state('');
  let writing = $state(false);
  let playing = $state(false);
  let index = $state(0);
  let speed = $state(1);
  let transcript = $state(false);
  let loading = $state(false);
  const SPEEDS = [0.9, 1, 1.15, 1.3, 1.5];

  let element: HTMLAudioElement | null = null;
  let utteranceToken = 0;

  const lines = $derived(audio?.lines ?? []);
  const total = $derived(lines.length);
  const current = $derived(lines[index]);
  /** About 150 spoken words a minute, so a listener knows what they are in for. */
  const minutes = $derived(Math.max(1, Math.round(lines.reduce((n, l) => n + l.text.split(/\s+/).length, 0) / 150)));
  const remaining = $derived(Math.max(0, Math.round(lines.slice(index).reduce((n, l) => n + l.text.split(/\s+/).length, 0) / 150)));
  const phase = $derived(!audio ? 'none' : audio.status);

  onMount(() => {
    void load();
    const stops: Array<() => void> = [];
    void onEvent<{ session_id: string; status: string }>('audio:state', (event) => {
      if (event.session_id === sessionId) void load();
    }).then((stop) => stops.push(stop));
    return () => { stop(); stops.forEach((s) => s()); };
  });

  async function load() {
    loading = true;
    try { audio = await api.getLessonAudio(sessionId); }
    catch (e) { error = String(e); audio = null; }
    finally { loading = false; }
  }
  async function write() {
    writing = true; error = '';
    try { await api.writeLessonAudio(sessionId); await load(); }
    catch (e) { error = String(e); }
    finally { writing = false; }
  }

  function stop() {
    playing = false;
    utteranceToken += 1;
    if (element) { element.pause(); element = null; }
    if (typeof speechSynthesis !== 'undefined') speechSynthesis.cancel();
  }
  function play() {
    if (!current) return;
    playing = true;
    speak(index);
  }
  function toggle() { playing ? stop() : play(); }
  function seek(to: number) {
    const was = playing;
    stop();
    index = Math.max(0, Math.min(total - 1, to));
    if (was) play();
  }
  function advance() {
    if (index + 1 >= total) { stop(); index = 0; return; }
    index += 1;
    speak(index);
  }
  function speak(at: number) {
    const line = lines[at];
    if (!line) { stop(); return; }
    if (line.file && !audio?.speak) {
      const next = new Audio(assetUrl(line.file));
      next.playbackRate = speed;
      next.onended = () => { if (playing && element === next) advance(); };
      next.onerror = () => { if (playing && element === next) { error = 'A segment did not play; reading it instead.'; readAloud(line.text, line.speaker, at); } };
      element = next;
      void next.play().catch(() => { error = 'The segment could not start; reading it instead.'; readAloud(line.text, line.speaker, at); });
      return;
    }
    readAloud(line.text, line.speaker, at);
  }
  function readAloud(text: string, speaker: string, at: number) {
    if (typeof speechSynthesis === 'undefined') { error = 'This webview has no speech voices.'; stop(); return; }
    const token = ++utteranceToken;
    speechSynthesis.cancel();
    const utterance = new SpeechSynthesisUtterance(text);
    utterance.rate = speed;
    const wanted = speaker === 'student' ? audio?.student_voice : audio?.teacher_voice;
    const voices = speechSynthesis.getVoices();
    const chosen = (wanted && voices.find((v) => v.name === wanted)) || voices.find((v) => v.lang.toLowerCase().startsWith('en') && (speaker === 'student' ? /female|samantha|karen|moira|tessa|zira/i.test(v.name) : /male|daniel|alex|david|george|mark/i.test(v.name))) || voices.find((v) => v.lang.toLowerCase().startsWith('en'));
    if (chosen) utterance.voice = chosen;
    utterance.onend = () => { if (playing && token === utteranceToken && index === at) advance(); };
    utterance.onerror = () => { if (playing && token === utteranceToken) advance(); };
    speechSynthesis.speak(utterance);
  }
  function setSpeed(next: number) {
    speed = next;
    if (element) element.playbackRate = next;
    else if (playing) { const at = index; stop(); index = at; play(); }
  }
  function close() { stop(); onclose(); }
</script>

<div class="listen" role="region" aria-label="Listen to this lesson">
  <header class="listen-head">
    <span class="mono kicker"><AudioLines size={11} /> LISTENING</span>
    {#if audio?.status === 'ready' && total}<span class="mono meta">{audio.engine === 'system' || audio.speak ? 'read by the desk' : audio.engine === 'piper' ? 'Piper' : 'Kokoro'} · about {minutes} min · {remaining} left</span>{/if}
    <button type="button" class="icon" onclick={close} aria-label="Close the player"><X size={13} /></button>
  </header>

  {#if audio === undefined || (loading && !audio)}
    <p class="line mono"><Loader size={11} class="spin" /> looking for this lesson's audio…</p>
  {:else if phase === 'none' || (phase === 'failed' && total === 0)}
    <div class="empty">
      <p>{phase === 'failed' ? `The conversation was not written: ${audio?.error ?? 'unknown reason'}.` : 'No audio for this lesson yet. The tutor can write it now as a conversation between a teacher and a student, voiced with the class\'s chosen voices; it takes a minute or two and the lesson stays as it is.'}</p>
      <button type="button" class="cta mono-cta" disabled={writing} onclick={write}><PenLine size={12} /> {writing ? 'starting…' : phase === 'failed' ? 'Try again' : 'Write the audio'}</button>
    </div>
  {:else if phase === 'writing' || phase === 'rendering'}
    <p class="line mono"><Loader size={11} class="spin" /> {phase === 'writing' ? 'the tutor is writing the conversation…' : 'voicing the lines…'} <span class="faint">every step is on the Logs page</span></p>
  {:else}
    {#if audio?.status === 'failed'}<p class="note mono">{audio.error}</p>{/if}
    <div class="transport">
      <button type="button" class="icon" onclick={() => seek(index - 1)} disabled={index === 0} aria-label="Previous line"><SkipBack size={13} /></button>
      <button type="button" class="play" onclick={toggle} aria-label={playing ? 'Pause' : 'Play'}>{#if playing}<Pause size={15} />{:else}<Play size={15} />{/if}</button>
      <button type="button" class="icon" onclick={() => seek(index + 1)} disabled={index + 1 >= total} aria-label="Next line"><SkipForward size={13} /></button>
      <div class="now">
        <span class="mono who" class:student={current?.speaker === 'student'}>{current?.speaker === 'student' ? 'STUDENT' : 'TEACHER'} · {index + 1} / {total}</span>
        <p class="text">{current?.text}</p>
      </div>
      <div class="speeds mono" role="group" aria-label="Speed">
        {#each SPEEDS as s (s)}<button type="button" class:active={speed === s} onclick={() => setSpeed(s)}>{s}×</button>{/each}
      </div>
      <button type="button" class="icon" class:active={transcript} onclick={() => (transcript = !transcript)} aria-label="Transcript" aria-expanded={transcript}><ListMusic size={13} /></button>
    </div>
    <div class="progress" aria-hidden="true"><div class="fill" style:width={`${total ? ((index + (playing ? 0.5 : 0)) / total) * 100 : 0}%`}></div></div>
    {#if transcript}
      <ol class="transcript">
        {#each lines as line, at (at)}
          <li class:current={at === index} class:student={line.speaker === 'student'}>
            <button type="button" onclick={() => seek(at)}><span class="mono who">{line.speaker === 'student' ? 'S' : 'T'}</span><span>{line.text}</span></button>
          </li>
        {/each}
      </ol>
    {/if}
  {/if}
  {#if error}<p class="error mono" role="alert">{error}</p>{/if}
</div>

<style>
  .listen { position: sticky; bottom: 12px; z-index: 5; margin: 18px 0 0; padding: 12px 14px; background: color-mix(in srgb, var(--node-bg) 94%, transparent); backdrop-filter: blur(10px); border: 1px solid color-mix(in srgb, var(--accent) 40%, var(--node-border)); border-radius: var(--radius-panel); box-shadow: 0 14px 40px -18px color-mix(in srgb, var(--accent) 60%, transparent); }
  .listen-head { display: flex; align-items: center; gap: 12px; margin-bottom: 8px; }
  .kicker { display: inline-flex; align-items: center; gap: 6px; font-size: 9px; letter-spacing: 1.3px; color: var(--accent); }
  .meta { font-size: 9px; color: var(--faint); letter-spacing: 0.4px; }
  .listen-head .icon { margin-left: auto; }
  .icon { display: grid; place-items: center; width: 28px; height: 28px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--surface); color: var(--muted); cursor: pointer; }
  .icon:hover:not(:disabled) { color: var(--fg); } .icon:disabled { opacity: 0.4; cursor: default; } .icon.active { color: var(--accent); border-color: var(--accent); }
  .play { display: grid; place-items: center; width: 40px; height: 40px; border: 0; border-radius: 50%; background: var(--accent); color: var(--accent-fg); cursor: pointer; box-shadow: 0 0 16px color-mix(in srgb, var(--accent) 45%, transparent); }
  .transport { display: flex; align-items: center; gap: 10px; }
  .now { flex: 1; min-width: 0; }
  .who { font-size: 8.5px; letter-spacing: 1.1px; color: var(--accent); } .who.student { color: var(--violet); }
  .text { margin: 3px 0 0; font-size: 12.5px; line-height: 1.45; color: var(--fg); display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .speeds { display: inline-flex; gap: 2px; padding: 2px; border: 1px solid var(--node-border); border-radius: var(--radius-control); background: var(--surface); }
  .speeds button { border: 0; background: transparent; color: var(--muted); font: 9px var(--font-mono); padding: 4px 6px; border-radius: calc(var(--radius-control) - 2px); cursor: pointer; }
  .speeds button.active { background: color-mix(in srgb, var(--accent) 18%, var(--surface-2)); color: var(--accent); }
  .progress { height: 3px; margin-top: 10px; border-radius: 999px; background: var(--surface-2); overflow: hidden; } .fill { height: 100%; background: var(--accent); transition: width 300ms ease; }
  .transcript { list-style: none; margin: 10px 0 0; padding: 0; max-height: 240px; overflow: auto; border-top: 1px dashed var(--node-divider); }
  .transcript li button { display: flex; gap: 10px; width: 100%; text-align: left; padding: 7px 4px; border: 0; background: transparent; color: var(--muted); font-size: 11.5px; line-height: 1.5; cursor: pointer; border-radius: var(--radius-control); }
  .transcript li button:hover { background: var(--surface); } .transcript li.current button { color: var(--fg); background: color-mix(in srgb, var(--accent) 10%, transparent); }
  .transcript .who { flex: none; width: 14px; padding-top: 3px; } .transcript li.student .who { color: var(--violet); }
  .line { display: flex; align-items: center; gap: 8px; margin: 0; font-size: 10.5px; color: var(--muted); } .faint { color: var(--faint); }
  .empty { display: flex; flex-direction: column; gap: 10px; align-items: flex-start; } .empty p { margin: 0; font-size: 12px; line-height: 1.55; color: var(--muted); max-width: 70ch; }
  .note { margin: 0 0 8px; font-size: 9.5px; color: var(--led-warn); }
  .error { margin: 8px 0 0; font-size: 10px; color: var(--led-err); }
  :global(.spin) { animation: spin 1.1s linear infinite; } @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 720px) { .speeds { display: none; } }
</style>
