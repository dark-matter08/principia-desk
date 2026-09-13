<script lang="ts">
  /**
   * Listening, per class: whether each prepared lesson is also written as a
   * two-host conversation and voiced, with which engine and which voice for
   * the teacher and the student. Engines and voices are set up in Settings ›
   * Voices; this picks from what is there.
   */
  import { onMount } from 'svelte';
  import { AudioLines, Play, X, Loader } from 'lucide-svelte';
  import { api, type AudioStatus, type AudioVoiceStatus } from '../../ipc';
  import type { AudioPreference } from '../../contracts/enrollment';
  import { app } from '../../stores.svelte';
  import { assetUrl } from '../lessons/asset';
  import Dropdown from '../../components/Dropdown.svelte';

  let { courseId, label }: { courseId: string; label: string } = $props();

  let status = $state<AudioStatus | null>(null);
  let saved = $state<AudioPreference | null>(null);
  let draft = $state<AudioPreference>({ enabled: false, engine: 'system', teacher_voice: '', student_voice: '' });
  let saving = $state(false);
  let done = $state(false);
  let error = $state('');
  let systemVoices = $state<{ value: string; label: string }[]>([]);
  let playing = $state('');
  let player: HTMLAudioElement | null = null;

  const dirty = $derived(!!saved && JSON.stringify(draft) !== JSON.stringify(saved));
  const engineOptions = $derived((status?.engines ?? []).map((e) => ({ value: e.id, label: e.label + (e.ready ? '' : ' (not installed)'), disabled: !e.ready })));
  const voiceOptions = $derived.by(() => {
    if (draft.engine === 'system') return systemVoices.length ? systemVoices : [{ value: '', label: 'the default voice' }];
    const installed = (status?.voices ?? []).filter((v) => v.engine === draft.engine && v.installed);
    return installed.length ? installed.map((v) => ({ value: v.id, label: `${v.label} · ${v.language} · ${v.hint}` })) : [{ value: '', label: 'no voice downloaded yet: Settings › Voices' }];
  });
  const engineReady = $derived(status?.engines.find((e) => e.id === draft.engine)?.ready ?? draft.engine === 'system');

  onMount(() => {
    void load();
    readSystemVoices();
    if (typeof speechSynthesis !== 'undefined') speechSynthesis.addEventListener('voiceschanged', readSystemVoices);
    return () => { player?.pause(); if (typeof speechSynthesis !== 'undefined') { speechSynthesis.removeEventListener('voiceschanged', readSystemVoices); speechSynthesis.cancel(); } };
  });

  function readSystemVoices() {
    if (typeof speechSynthesis === 'undefined') return;
    const voices = speechSynthesis.getVoices().filter((v) => v.lang.toLowerCase().startsWith('en'));
    systemVoices = voices.map((v) => ({ value: v.name, label: `${v.name} · ${v.lang}` }));
    // A class on the system voice that never chose gets the first two, so the
    // pair is visible and can be heard before saving.
    if (draft.engine === 'system' && !draft.teacher_voice && systemVoices.length) {
      draft.teacher_voice = systemVoices[0].value;
      draft.student_voice = systemVoices[1]?.value ?? systemVoices[0].value;
      if (saved && !saved.teacher_voice) saved = { ...saved, teacher_voice: draft.teacher_voice, student_voice: draft.student_voice };
    }
  }
  async function load() {
    try {
      const [audio, preference] = await Promise.all([api.getAudioStatus(), api.getClassAudio(courseId)]);
      status = audio;
      saved = { ...preference, engine: preference.engine || 'system' };
      draft = { ...saved };
      readSystemVoices();
    } catch (e) { error = String(e); }
  }
  function chooseEngine(engine: string) {
    draft.engine = engine as AudioPreference['engine'];
    draft.teacher_voice = '';
    draft.student_voice = '';
    // Two voices that differ, when the engine has two.
    const installed = (status?.voices ?? []).filter((v) => v.engine === engine && v.installed);
    if (installed[0]) draft.teacher_voice = installed[0].id;
    if (installed[1]) draft.student_voice = installed[1].id;
    else if (installed[0]) draft.student_voice = installed[0].id;
    if (engine === 'system' && systemVoices.length) { draft.teacher_voice = systemVoices[0].value; draft.student_voice = systemVoices[1]?.value ?? systemVoices[0].value; }
  }
  async function save() {
    if (saving || !dirty) return;
    saving = true; error = ''; done = false;
    try {
      const next = $state.snapshot(draft);
      saved = { ...(await api.setClassAudio(courseId, next)) };
      if (!saved.engine) saved.engine = 'system';
      draft = { ...saved };
      done = true;
      await app.refresh();
    } catch (e) { error = String(e); }
    finally { saving = false; }
  }
  async function hear(voice: string) {
    if (!voice) return;
    if (playing === voice) { player?.pause(); if (typeof speechSynthesis !== 'undefined') speechSynthesis.cancel(); playing = ''; return; }
    error = '';
    if (draft.engine === 'system') {
      if (typeof speechSynthesis === 'undefined') return;
      speechSynthesis.cancel();
      const line = new SpeechSynthesisUtterance('A cache is a sticky note on the fridge; the database is the filing cabinet in the basement.');
      const chosen = speechSynthesis.getVoices().find((v) => v.name === voice);
      if (chosen) line.voice = chosen;
      line.onend = () => { playing = ''; };
      playing = voice;
      speechSynthesis.speak(line);
      return;
    }
    try {
      playing = `…${voice}`;
      const path = await api.previewVoice(draft.engine, voice);
      player?.pause();
      player = new Audio(assetUrl(path));
      player.onended = () => { playing = ''; };
      playing = voice;
      await player.play();
    } catch (e) { error = String(e); playing = ''; }
  }
  const voiceLabel = (id: string) => (status?.voices ?? []).find((v) => v.id === id)?.label ?? id;
  const summary = $derived.by(() => {
    const kept = saved;
    if (!kept || !kept.enabled) return 'Off';
    const engine = status?.engines.find((e) => e.id === (kept.engine || 'system'))?.label ?? kept.engine;
    const voices = kept.teacher_voice ? ` · ${voiceLabel(kept.teacher_voice)} and ${voiceLabel(kept.student_voice || kept.teacher_voice)}` : '';
    return `On · ${engine}${voices}`;
  });
  const readyVoices = $derived((status?.voices ?? []).filter((v: AudioVoiceStatus) => v.engine === draft.engine && v.installed).length);
</script>

<section class="listening" aria-labelledby="listening-heading">
  <h4 id="listening-heading"><AudioLines size={14} /> Listening</h4>
  <p>Have each {label} lesson also written as a conversation between a teacher and a student, voiced so you can listen with your hands busy. The audio follows the lesson: it is written once the lesson is ready and never delays it. Engines and voices are set up in Settings › Voices.</p>

  {#if !status || !saved}
    <p class="note mono"><Loader size={11} class="spin" /> reading the voices on this machine…</p>
  {:else}
    <label class="toggle">
      <input type="checkbox" bind:checked={draft.enabled} />
      <span class="switch" aria-hidden="true"></span>
      <span class="toggle-text"><strong>Write the audio with every lesson</strong><small>{draft.enabled ? 'On: the next lesson prepared gets its conversation and voice.' : 'Off: a lesson can still be voiced from its page, one at a time.'}</small></span>
    </label>

    <div class="fields" class:muted={!draft.enabled}>
      <Dropdown label="Engine" value={draft.engine} options={engineOptions} onchange={(value: string) => chooseEngine(value)} />
      <div class="voice-field">
        <Dropdown label="Teacher's voice" bind:value={draft.teacher_voice} options={voiceOptions} />
        {#if draft.teacher_voice}<button type="button" class="ghost mono-ghost small" onclick={() => hear(draft.teacher_voice)} aria-label="Hear the teacher's voice">{#if playing === draft.teacher_voice}<X size={11} />{:else if playing === `…${draft.teacher_voice}`}<Loader size={11} class="spin" />{:else}<Play size={11} />{/if}</button>{/if}
      </div>
      <div class="voice-field">
        <Dropdown label="Student's voice" bind:value={draft.student_voice} options={voiceOptions} />
        {#if draft.student_voice}<button type="button" class="ghost mono-ghost small" onclick={() => hear(draft.student_voice)} aria-label="Hear the student's voice">{#if playing === draft.student_voice}<X size={11} />{:else if playing === `…${draft.student_voice}`}<Loader size={11} class="spin" />{:else}<Play size={11} />{/if}</button>{/if}
      </div>
    </div>
    {#if draft.engine !== 'system' && !engineReady}<p class="note mono warn">{status.engines.find((e) => e.id === draft.engine)?.label} is not installed on this machine; the desk reads the lines itself until it is.</p>
    {:else if draft.engine !== 'system' && readyVoices === 0}<p class="note mono warn">No {status.engines.find((e) => e.id === draft.engine)?.label} voice is downloaded yet. Settings › Voices has them.</p>{/if}

    <div class="actions"><span role="status">{saving ? 'Saving…' : dirty ? 'Unsaved listening change' : done ? 'Listening saved' : summary}</span><button type="button" class="cta mono-cta" disabled={saving || !dirty} onclick={save}>Save listening</button></div>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .listening { max-width: 1000px; margin: 28px auto 0; border: 1px solid var(--node-border); border-radius: var(--radius-panel); padding: 18px; }
  h4 { display: flex; align-items: center; gap: 8px; margin: 0 0 8px; font-size: 14px; font-weight: 500; }
  p { color: var(--muted); font-size: 13px; line-height: 1.6; margin: 0 0 14px; }
  .toggle { display: flex; align-items: center; gap: 12px; cursor: pointer; margin-bottom: 16px; }
  .toggle input { position: absolute; opacity: 0; width: 1px; height: 1px; }
  .switch { position: relative; flex: none; width: 34px; height: 18px; border-radius: 999px; background: var(--surface-2); border: 1px solid var(--node-border); transition: background 140ms ease; }
  .switch::after { content: ''; position: absolute; top: 2px; left: 2px; width: 12px; height: 12px; border-radius: 50%; background: var(--muted); transition: transform 140ms ease, background 140ms ease; }
  .toggle input:checked + .switch { background: color-mix(in srgb, var(--accent) 35%, var(--surface-2)); border-color: var(--accent); }
  .toggle input:checked + .switch::after { transform: translateX(16px); background: var(--accent); }
  .toggle input:focus-visible + .switch { outline: 2px solid var(--accent); outline-offset: 2px; }
  .toggle-text { display: flex; flex-direction: column; gap: 2px; } .toggle-text strong { font-size: 12.5px; font-weight: 500; } .toggle-text small { font-size: 10.5px; color: var(--muted); }
  .fields { display: grid; grid-template-columns: minmax(160px, 1fr) minmax(200px, 1.4fr) minmax(200px, 1.4fr); gap: 16px; align-items: end; transition: opacity 140ms ease; }
  .fields.muted { opacity: 0.7; }
  .voice-field { display: flex; align-items: flex-end; gap: 6px; min-width: 0; } .voice-field :global(.dropdown) { flex: 1; min-width: 0; }
  .note { margin: 12px 0 0; font-size: 10px; display: flex; align-items: center; gap: 6px; } .note.warn { color: var(--led-warn); }
  .actions { display: flex; gap: 12px; align-items: center; justify-content: space-between; margin-top: 16px; } .actions span { color: var(--muted); font: 11px var(--font-mono); }
  .error { margin-top: 14px; color: var(--led-err); overflow-wrap: anywhere; }
  :global(.spin) { animation: spin 1.1s linear infinite; } @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 800px) { .fields { grid-template-columns: 1fr; } }
</style>
