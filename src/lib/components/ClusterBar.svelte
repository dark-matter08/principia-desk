<script lang="ts">
  let {
    route,
    status = 'all systems nominal',
    tone = 'ok',
  }: { route: string; status?: string; tone?: 'ok' | 'warn' | 'err' | 'idle' } = $props();

  const toneColor = {
    ok: 'var(--led-ok)',
    warn: 'var(--led-warn)',
    err: 'var(--led-err)',
    idle: 'var(--led-idle)',
  };
</script>

<div class="cluster-bar">
  <span class="path">principia://{route} <span class="dim">·</span> cluster: local <span class="dim">·</span> region: home-1</span>
  <span class="status" style="color: {toneColor[tone]};">● {status}</span>
</div>

<style>
  .cluster-bar {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    padding: 14px 28px 0;
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 1px;
  }
  .path {
    color: var(--faint);
  }
  .dim {
    color: var(--border);
  }
  @media (max-width: 720px) {
    .cluster-bar { padding: 12px 18px 0; flex-wrap: wrap; gap: 5px 14px; font-size: 9px; letter-spacing: 0.5px; }
  }
</style>
