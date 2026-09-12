<script lang="ts">
  /**
   * The screen blocker that teaches: a live traffic simulation you can break.
   * Click nodes to kill/heal them and watch retries, failover, cache misses,
   * and the DLQ fill up. Runs standalone on secondary displays during lock.
   */
  import { onMount } from 'svelte';

  type NodeId = 'lb' | 'web1' | 'web2' | 'web3' | 'cache' | 'db';
  interface SimNode {
    id: NodeId;
    label: string;
    x: number;
    y: number;
    up: boolean;
    load: number; // recent packets, decays
  }

  let nodes = $state<Record<NodeId, SimNode>>({
    lb: { id: 'lb', label: 'load-balancer', x: 250, y: 360, up: true, load: 0 },
    web1: { id: 'web1', label: 'web-1', x: 480, y: 160, up: true, load: 0 },
    web2: { id: 'web2', label: 'web-2', x: 480, y: 360, up: true, load: 0 },
    web3: { id: 'web3', label: 'web-3', x: 480, y: 560, up: true, load: 0 },
    cache: { id: 'cache', label: 'cache', x: 730, y: 250, up: true, load: 0 },
    db: { id: 'db', label: 'postgres', x: 730, y: 470, up: true, load: 0 },
  });

  interface Packet {
    path: { x: number; y: number }[];
    t: number; // 0..1 along the whole path
    speed: number;
    ok: boolean;
    dropped: boolean;
  }
  let packets = $state<Packet[]>([]);
  let served = $state(0);
  let dlq = $state(0);
  let cacheMisses = $state(0);
  let ttl = $state<number | null>(null);

  const CLIENT = { x: 70, y: 360 };

  function healthyWebs(): SimNode[] {
    return (['web1', 'web2', 'web3'] as NodeId[]).map((i) => nodes[i]).filter((n) => n.up);
  }

  function spawn() {
    if (!nodes.lb.up) {
      // No LB: requests die at the front door.
      packets.push({ path: [CLIENT, { x: nodes.lb.x, y: nodes.lb.y }], t: 0, speed: 0.012, ok: false, dropped: true });
      return;
    }
    const webs = healthyWebs();
    if (webs.length === 0) {
      // Breaker open: fail fast at the LB.
      packets.push({ path: [CLIENT, { x: nodes.lb.x, y: nodes.lb.y }], t: 0, speed: 0.012, ok: false, dropped: true });
      return;
    }
    const web = webs[Math.floor(Math.random() * webs.length)];
    web.load = Math.min(web.load + 1 / webs.length, 6);
    // 70% of reads hit cache when it's up; otherwise everything goes to the db.
    const useCache = nodes.cache.up && Math.random() < 0.7;
    if (!nodes.cache.up) cacheMisses++;
    const store = useCache ? nodes.cache : nodes.db;
    if (!useCache) nodes.db.load = Math.min(nodes.db.load + 1, 8);
    const storeOk = store.up;
    const path = [
      CLIENT,
      { x: nodes.lb.x, y: nodes.lb.y },
      { x: web.x, y: web.y },
      { x: store.x, y: store.y },
    ];
    packets.push({ path, t: 0, speed: 0.008 + Math.random() * 0.004, ok: storeOk, dropped: !storeOk });
  }

  function pos(p: Packet): { x: number; y: number } {
    const segs = p.path.length - 1;
    const ft = Math.min(p.t, 0.9999) * segs;
    const i = Math.floor(ft);
    const lt = ft - i;
    const a = p.path[i];
    const b = p.path[i + 1];
    return { x: a.x + (b.x - a.x) * lt, y: a.y + (b.y - a.y) * lt };
  }

  function toggle(id: NodeId) {
    nodes[id].up = !nodes[id].up;
  }

  onMount(() => {
    const sim = setInterval(() => {
      if (Math.random() < 0.75) spawn();
      for (const p of packets) p.t += p.speed;
      for (const f of packets.filter((p) => p.t >= 1)) {
        if (f.ok) served++;
        else dlq++;
      }
      packets = packets.filter((p) => p.t < 1);
      for (const n of Object.values(nodes)) n.load *= 0.97;
    }, 40);

    // TTL from the main window's authoritative timer (only inside Tauri).
    let unlisten: (() => void) | undefined;
    if ('__TAURI_INTERNALS__' in window) {
      import('@tauri-apps/api/event').then(({ listen }) => {
        listen<number>('timer:tick', (e) => (ttl = e.payload)).then((u) => (unlisten = u));
      });
    }
    return () => {
      clearInterval(sim);
      unlisten?.();
    };
  });

  const mmss = $derived(
    ttl == null ? null : `${Math.floor(ttl / 60)}:${String(ttl % 60).padStart(2, '0')}`
  );
  const breakerOpen = $derived(healthyWebs().length === 0 || !nodes.lb.up);
</script>

<div class="lab blueprint theme-noir">
  <div class="lab-top mono">
    <span>sdr://chaos-lab · this display is sealed; the session runs on your main screen</span>
    {#if mmss}<span class="ttl">TTL {mmss}</span>{/if}
  </div>

  <div class="lab-head">
    <h1>Chaos lab</h1>
    <p class="mono sub">click a node to kill it. watch what your traffic does. click again to heal.</p>
  </div>

  <svg viewBox="0 0 900 720" class="topo-svg">
    <!-- pipes -->
    {#each [nodes.web1, nodes.web2, nodes.web3] as w}
      <line x1={nodes.lb.x} y1={nodes.lb.y} x2={w.x} y2={w.y} class="pipe" class:dead={!w.up || !nodes.lb.up} />
      <line x1={w.x} y1={w.y} x2={nodes.cache.x} y2={nodes.cache.y} class="pipe" class:dead={!w.up || !nodes.cache.up} />
      <line x1={w.x} y1={w.y} x2={nodes.db.x} y2={nodes.db.y} class="pipe" class:dead={!w.up || !nodes.db.up} />
    {/each}
    <line x1={CLIENT.x} y1={CLIENT.y} x2={nodes.lb.x} y2={nodes.lb.y} class="pipe" />
    <line x1={nodes.cache.x} y1={nodes.cache.y} x2={nodes.db.x} y2={nodes.db.y} class="pipe faint" />

    <!-- packets -->
    {#each packets as p}
      {@const c = pos(p)}
      <circle cx={c.x} cy={c.y} r="4" class="pkt" class:bad={p.dropped} />
    {/each}

    <!-- client -->
    <g class="client">
      <rect x={CLIENT.x - 44} y={CLIENT.y - 24} width="88" height="48" rx="8" />
      <text x={CLIENT.x} y={CLIENT.y + 4} text-anchor="middle">clients</text>
    </g>

    <!-- nodes -->
    {#each Object.values(nodes) as n}
      <g
        class="node"
        class:down={!n.up}
        onclick={() => toggle(n.id)}
        onkeydown={(e) => e.key === 'Enter' && toggle(n.id)}
        role="button"
        tabindex="0"
      >
        <rect x={n.x - 62} y={n.y - 30} width="124" height="60" rx="9" style="stroke-width: {1 + Math.min(n.load, 5)}px" />
        <circle cx={n.x - 46} cy={n.y - 14} r="4" class="led" />
        <text x={n.x + 4} y={n.y - 8} text-anchor="middle" class="nlabel">{n.label}</text>
        <text x={n.x} y={n.y + 14} text-anchor="middle" class="nstate">{n.up ? 'healthy' : 'KILLED'}</text>
      </g>
    {/each}
  </svg>

  <div class="lab-stats mono">
    <span class="stat ok">200 OK: {served}</span>
    <span class="stat err">DLQ: {dlq}</span>
    <span class="stat warn">cache misses: {cacheMisses}</span>
    <span class="stat" class:err={breakerOpen}>{breakerOpen ? 'CIRCUIT BREAKER OPEN · failing fast' : 'breaker: closed'}</span>
  </div>
</div>

<style>
  .lab {
    position: fixed;
    inset: 0;
    background: var(--bg);
    color: var(--fg);
    display: flex;
    flex-direction: column;
    user-select: none;
    cursor: default;
  }
  .lab-top {
    display: flex;
    justify-content: space-between;
    padding: 12px 20px;
    font-size: 11px;
    color: var(--faint);
    border-bottom: 1px solid var(--node-divider);
  }
  .ttl {
    color: var(--accent);
  }
  .lab-head {
    text-align: center;
    padding: 22px 0 0;
  }
  .lab-head h1 {
    font-size: 30px;
  }
  .sub {
    font-size: 11px;
    color: var(--muted);
    margin-top: 4px;
  }
  .topo-svg {
    flex: 1;
    width: min(92vw, 1200px);
    margin: 0 auto;
    min-height: 0;
  }
  .pipe {
    stroke: var(--node-border);
    stroke-width: 1.5;
    stroke-dasharray: 5 4;
  }
  .pipe.dead {
    stroke: #3a2228;
    opacity: 0.4;
  }
  .pipe.faint {
    opacity: 0.3;
  }
  .pkt {
    fill: var(--led-ok);
  }
  .pkt.bad {
    fill: var(--led-err);
  }
  .client rect {
    fill: var(--surface);
    stroke: var(--node-border);
  }
  .client text {
    font-family: var(--font-mono);
    font-size: 12px;
    fill: var(--muted);
  }
  .node {
    cursor: pointer;
    outline: none;
  }
  .node rect {
    fill: var(--node-bg);
    stroke: var(--led-ok);
    transition: stroke 0.2s ease;
  }
  .node:hover rect {
    fill: var(--surface-2);
  }
  .node .led {
    fill: var(--led-ok);
  }
  .node .nlabel {
    font-family: var(--font-mono);
    font-size: 13px;
    fill: var(--fg);
  }
  .node .nstate {
    font-family: var(--font-mono);
    font-size: 9px;
    letter-spacing: 1px;
    fill: var(--faint);
  }
  .node.down rect {
    stroke: var(--led-err);
    fill: #1f1316;
  }
  .node.down .led {
    fill: var(--led-err);
    animation: led-pulse 1s ease infinite;
  }
  .node.down .nstate {
    fill: var(--led-err);
  }
  .lab-stats {
    display: flex;
    gap: 26px;
    justify-content: center;
    padding: 14px 0 22px;
    font-size: 12px;
    color: var(--muted);
  }
  .stat.ok {
    color: var(--led-ok);
  }
  .stat.err {
    color: var(--led-err);
  }
  .stat.warn {
    color: var(--led-warn);
  }
</style>
