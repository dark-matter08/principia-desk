<script lang="ts">
  /**
   * Updates from inside the desk. The desk reads the release manifest after
   * boot and every few hours; this card says what it found and installs
   * only when asked. The download is verified against the key the release
   * pipeline signs with before anything runs.
   */
  import { onMount } from 'svelte';
  import { RefreshCw, Download, ExternalLink, Check } from 'lucide-svelte';
  import { api, onEvent, type UpdateStatus, type UpdateProgress } from '$lib/ipc';
  import { app } from '$lib/stores.svelte';
  import { PLATFORM } from '$lib/platform';
  import NodeCard from '$lib/components/NodeCard.svelte';

  const RELEASES = 'https://github.com/dark-matter08/principia-desk/releases/latest';

  let status = $state<UpdateStatus | null>(null);
  let busy = $state('');
  let error = $state('');
  let progress = $state<UpdateProgress | null>(null);
  let downloaded = $state(false);

  const when = (iso: string | null) => {
    if (!iso) return 'never';
    const date = new Date(iso);
    return Number.isNaN(date.getTime()) ? iso : date.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
  };
  const megabytes = (bytes: number) => `${(bytes / 1_048_576).toFixed(1)} MB`;
  const percent = $derived(progress?.total ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100)) : null);
  const badge = $derived(!status ? '…' : status.installing || busy === 'install' ? 'installing' : status.available ? `${status.available.version} available` : status.error ? 'check failed' : 'up to date');
  const tone = $derived(!status ? 'muted' : status.available ? 'amber' : status.error ? 'red' : 'teal');

  onMount(() => {
    void load();
    const stops: Array<() => void> = [];
    void onEvent<UpdateProgress>('update:progress', (p) => { progress = p; }).then((stop) => stops.push(stop));
    void onEvent('update:downloaded', () => { downloaded = true; }).then((stop) => stops.push(stop));
    void onEvent('update:available', () => void load()).then((stop) => stops.push(stop));
    return () => stops.forEach((stop) => stop());
  });

  async function load() {
    try { status = await api.getUpdateStatus(); } catch (e) { error = String(e); }
  }
  async function check() {
    busy = 'check'; error = '';
    try { status = await api.checkForUpdate(); app.update = status.available; }
    catch (e) { error = String(e); }
    finally { busy = ''; }
  }
  async function install() {
    busy = 'install'; error = ''; progress = null; downloaded = false;
    try {
      // Resolves only on failure: on success the desk relaunches itself.
      await api.installUpdate();
    } catch (e) { error = String(e); busy = ''; downloaded = false; }
  }
</script>

<NodeCard Icon={RefreshCw} name="updates" {badge} badgeTone={tone}>
  <div class="head"><strong>The desk keeps itself current</strong><p>Every release publishes a signed manifest beside its installers. The desk reads it after it starts and every six hours, and installs a newer version only when you ask, after checking the signature. Your profile and its history stay where they are.</p></div>

  <div class="row">
    <div class="versions mono">
      <span class="k">RUNNING</span><span class="v">{status?.current ?? '…'}</span>
      <span class="k">LAST CHECK</span><span class="v">{status ? when(status.checked_at) : '…'}</span>
      {#if status?.available}<span class="k">AVAILABLE</span><span class="v lit">{status.available.version}{#if status.available.date}{' · '}{when(status.available.date)}{/if}</span>{/if}
    </div>
    <div class="keys">
      <button type="button" class="ghost mono-ghost" disabled={!!busy || !!status?.installing} onclick={check}><RefreshCw size={11} /> {busy === 'check' ? 'checking…' : 'check now'}</button>
      {#if status?.available}
        <button type="button" class="cta mono-cta" disabled={!!busy || !!status.installing || !!app.state?.focus} onclick={install}><Download size={12} /> {busy === 'install' ? (downloaded ? 'installing…' : 'downloading…') : `Update to ${status.available.version} and relaunch`}</button>
      {/if}
    </div>
  </div>

  {#if status?.available?.notes}
    <div class="notes"><span class="mono notes-h">WHAT CHANGED</span><p>{status.available.notes}</p></div>
  {/if}

  {#if busy === 'install'}
    <div class="progress" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={percent ?? undefined}>
      <div class="bar"><div class="fill" style:width={`${downloaded ? 100 : (percent ?? 8)}%`} class:indeterminate={!downloaded && percent === null}></div></div>
      <span class="mono">{downloaded ? (PLATFORM === 'linux' ? 'installing: the package manager may ask for your password' : 'installing…') : progress?.total ? `${megabytes(progress.downloaded)} of ${megabytes(progress.total)}` : progress ? megabytes(progress.downloaded) : 'connecting…'}</span>
    </div>
  {/if}

  {#if app.state?.focus}<p class="note mono">Finish, pause or skip the active session before updating.</p>{/if}
  {#if status?.error && !status.available}<p class="note mono">The last check did not get through: {status.error}</p>{/if}
  <p class="note mono">Installers for every platform are on <a href={RELEASES} target="_blank" rel="noreferrer">the releases page <ExternalLink size={9} /></a>, for a machine that cannot update itself; opening a newer installer over this one keeps the profile too.</p>
  {#if status && !status.available && !status.error && status.checked_at}<p class="done mono"><Check size={11} /> This is the latest version.</p>{/if}
  {#if error}<p class="error mono" role="alert">{error}</p>{/if}
</NodeCard>

<style>
  .head { margin-bottom: 14px; } .head strong { display: block; font-size: 14px; font-weight: 500; } .head p { margin: 5px 0 0; font-size: 11px; color: var(--muted); line-height: 1.6; max-width: 76ch; }
  .row { display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; flex-wrap: wrap; }
  .versions { display: grid; grid-template-columns: max-content max-content; gap: 6px 14px; font-size: 10.5px; }
  .k { color: var(--faint); letter-spacing: 1.1px; font-size: 8.5px; align-self: center; } .v { color: var(--fg); } .v.lit { color: var(--accent); }
  .keys { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .notes { margin-top: 14px; padding: 10px 12px; border: 1px dashed var(--node-divider); border-radius: var(--radius-control); }
  .notes-h { display: block; font-size: 8.5px; letter-spacing: 1.1px; color: var(--faint); margin-bottom: 4px; }
  .notes p { margin: 0; font-size: 11px; line-height: 1.6; color: var(--muted); white-space: pre-line; }
  .progress { display: flex; flex-direction: column; gap: 6px; margin-top: 14px; }
  .progress span { font-size: 9.5px; color: var(--muted); }
  .bar { height: 6px; border-radius: 999px; background: var(--surface-2); overflow: hidden; border: 1px solid var(--node-border); }
  .fill { height: 100%; background: linear-gradient(90deg, var(--accent), color-mix(in srgb, var(--accent) 60%, var(--led-ok))); transition: width 200ms ease; }
  .fill.indeterminate { width: 30% !important; animation: slide 1.2s ease-in-out infinite; }
  @keyframes slide { 0% { transform: translateX(-100%); } 100% { transform: translateX(340%); } }
  .note { margin: 10px 0 0; font-size: 9.5px; line-height: 1.6; color: var(--muted); }
  .note a { display: inline-flex; align-items: center; gap: 3px; color: var(--accent); text-decoration: none; }
  .done { display: flex; align-items: center; gap: 6px; margin: 10px 0 0; font-size: 9.5px; color: var(--led-ok); }
  .error { color: var(--led-err); font-size: 10.5px; margin-top: 10px; overflow-wrap: anywhere; }
  @media (prefers-reduced-motion: reduce) { .fill.indeterminate { animation: none; } }
</style>
