<script lang="ts">
  /**
   * Topic selection as a LEADER ELECTION over the shard rack — the gamified
   * replacement for the casino wheel. Dispatch starts an election: shards are
   * eliminated round by round (✗ 503, red LED) while the term counter climbs;
   * the last two duel under an alternating scanner; the survivor is elected
   * as today's topic. The winner is pre-decided (chosenIndex) — the election
   * is theater with a guaranteed outcome, exactly like the wheel was.
   */
  import { Zap, Crown, X, ArrowDown, CircleCheck } from 'lucide-svelte';

  let {
    pool,
    chosenIndex,
    lockedCount = 0,
    onLanded,
  }: { pool: string[]; chosenIndex: number; lockedCount?: number; onLanded: () => void } = $props();

  type ShardState = 'in' | 'out' | 'scan' | 'winner';
  let states = $state<ShardState[]>([]);
  let term = $state(0);
  let phase = $state<'idle' | 'election' | 'duel' | 'done'>('idle');
  let burst = $state(false); // packet-burst celebration on the winner

  $effect(() => {
    if (phase === 'idle') states = pool.map(() => 'in');
  });

  export function spin() {
    if (phase !== 'idle') return;
    phase = 'election';

    // Elimination order: everyone except the chosen, shuffled.
    const losers = pool.map((_, i) => i).filter((i) => i !== chosenIndex);
    for (let i = losers.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [losers[i], losers[j]] = [losers[j], losers[i]];
    }
    const finalist = losers.pop()!; // survives until the duel

    // Schedule eliminations: quick early, slowing as the field thins.
    let t = 600;
    losers.forEach((idx, k) => {
      const interval = 240 + (k / Math.max(losers.length - 1, 1)) * 420;
      t += interval;
      const myTerm = k + 1;
      setTimeout(() => {
        states[idx] = 'out';
        term = myTerm;
      }, t);
    });

    // The duel: scanner alternates between the last two, slowing, then the
    // rival drops and the chosen shard takes the crown.
    t += 700;
    const duelStart = t;
    const flips = 7;
    setTimeout(() => (phase = 'duel'), duelStart - 400);
    for (let f = 0; f < flips; f++) {
      t += 260 + f * 110;
      const who = f % 2 === 0 ? chosenIndex : finalist;
      const other = f % 2 === 0 ? finalist : chosenIndex;
      setTimeout(() => {
        states[who] = 'scan';
        states[other] = 'in';
      }, t);
    }
    t += 650;
    setTimeout(() => {
      states[finalist] = 'out';
      states[chosenIndex] = 'winner';
      term += 1;
      phase = 'done';
      burst = true;
      setTimeout(() => (burst = false), 1400);
      setTimeout(onLanded, 500);
    }, t);
  }

  const remaining = $derived(states.filter((s) => s !== 'out').length);
  const stubCount = $derived(Math.min(lockedCount, 6));
</script>

<div class="router">
  <div class="ingress mono" class:active={phase === 'election' || phase === 'duel'}>
    {#if phase === 'idle'}
      <span class="ing-label"><ArrowDown size={10} /> {pool.length} shards standing for election</span>
    {:else if phase === 'duel'}
      <span class="ing-label duel"><Zap size={10} /> FINAL ROUND · TERM {term} · 2 candidates</span>
    {:else if phase === 'done'}
      <span class="ing-label done"><Crown size={10} /> LEADER ELECTED · TERM {term}</span>
    {:else}
      <span class="ing-label live"><Zap size={10} /> ELECTION · TERM {term} · {remaining} candidates remain</span>
    {/if}
    <svg class="ing-pipe" viewBox="0 0 100 26" preserveAspectRatio="none" aria-hidden="true">
      <path d="M 50 0 L 50 26" />
    </svg>
  </div>

  <div class="rack">
    {#each pool as title, i}
      <div
        class="shard mono"
        class:out={states[i] === 'out'}
        class:scan={states[i] === 'scan'}
        class:winner={states[i] === 'winner'}
      >
        {#if states[i] === 'winner' && burst}
          <div class="burst" aria-hidden="true">
            {#each Array(10) as _, p}
              <span class="pdot" style="--a: {p * 36}deg; --d: {28 + (p % 3) * 12}px"></span>
            {/each}
          </div>
        {/if}
        <div class="shard-top">
          <span class="led"></span>
          <span class="sid">shard-{String(i).padStart(2, '0')}</span>
          {#if states[i] === 'out'}<span class="verdict"><X size={9} /> 503</span>{/if}
          {#if states[i] === 'winner'}<span class="verdict win"><Crown size={9} /> leader</span>{/if}
        </div>
        <div class="stitle">{title}</div>
      </div>
    {/each}
    {#each Array(stubCount) as _, i}
      <div class="shard stub mono" class:out={phase === 'done'}>
        <div class="shard-top">
          <span class="led idle"></span>
          <span class="sid">shard-{String(pool.length + i).padStart(2, '0')}</span>
        </div>
        <div class="stitle">provisioning…</div>
      </div>
    {/each}
  </div>
</div>

<style>
  .router {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: min(880px, 94vw);
  }
  .ingress {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    min-height: 40px;
  }
  .ing-label {
    font-size: 10px;
    letter-spacing: 1.5px;
    text-transform: uppercase;
    color: var(--faint);
  }
  .ing-label.live {
    color: var(--accent);
  }
  .ing-label.duel {
    color: var(--led-warn);
    animation: led-pulse 0.5s ease infinite;
  }
  .ing-label.done {
    color: var(--led-ok);
  }
  .ing-pipe {
    width: 60px;
    height: 22px;
  }
  .ing-pipe path {
    stroke: var(--node-border);
    stroke-width: 1.5;
    stroke-dasharray: 4 3;
  }
  .ingress.active .ing-pipe path {
    stroke: var(--accent);
    animation: pipe-flow 0.5s linear infinite;
  }
  @keyframes pipe-flow {
    to {
      stroke-dashoffset: -7;
    }
  }
  .rack {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 10px;
    width: 100%;
    margin-top: 8px;
  }
  .shard {
    position: relative;
    background: var(--node-bg);
    border: 1px solid var(--node-border);
    border-radius: var(--radius-panel);
    padding: 9px 12px 10px;
    transition: border-color 0.12s ease, background 0.12s ease, opacity 0.45s ease, transform 0.3s ease;
    min-height: 62px;
  }
  .shard-top {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-bottom: 5px;
  }
  .led {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--led-ok);
    transition: background 0.15s ease, box-shadow 0.15s ease;
    flex-shrink: 0;
  }
  .sid {
    font-size: 8.5px;
    letter-spacing: 1px;
    color: var(--faint);
    text-transform: uppercase;
  }
  .verdict {
    margin-left: auto;
    font-size: 8.5px;
    letter-spacing: 1px;
    color: var(--led-err);
  }
  .verdict.win {
    color: var(--accent);
  }
  .stitle {
    font-size: 11.5px;
    color: var(--muted);
    line-height: 1.4;
  }
  /* election states */
  .shard.out {
    opacity: 0.28;
    transform: scale(0.97);
    border-color: #3a2228;
    animation: knocked 0.3s ease;
  }
  .shard.out .led {
    background: var(--led-err);
  }
  .shard.out .stitle {
    text-decoration: line-through;
    text-decoration-color: var(--led-err);
  }
  @keyframes knocked {
    0% {
      transform: translateX(0) scale(1);
    }
    30% {
      transform: translateX(-4px) scale(1);
    }
    60% {
      transform: translateX(3px) scale(0.99);
    }
    100% {
      transform: translateX(0) scale(0.97);
    }
  }
  .shard.scan {
    border-color: var(--led-warn);
    background: var(--surface-2);
    transform: scale(1.03);
  }
  .shard.scan .led {
    background: var(--led-warn);
    box-shadow: 0 0 8px var(--led-warn);
  }
  .shard.winner {
    border-color: var(--accent);
    background: var(--surface-2);
    transform: scale(1.06);
    box-shadow: 0 0 0 1px var(--accent), 0 8px 36px rgba(239, 159, 39, 0.22);
  }
  .shard.winner .led {
    background: var(--led-ok);
    box-shadow: 0 0 8px var(--led-ok);
  }
  .shard.winner .stitle {
    color: var(--fg);
  }
  .shard.winner .sid {
    color: var(--accent);
  }
  /* packet-burst celebration */
  .burst {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .pdot {
    position: absolute;
    top: 50%;
    left: 50%;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--accent);
    animation: burst-fly 1.1s ease-out forwards;
    transform: rotate(var(--a)) translateX(0);
  }
  @keyframes burst-fly {
    to {
      transform: rotate(var(--a)) translateX(calc(var(--d) + 70px));
      opacity: 0;
    }
  }
  .shard.stub {
    border-style: dashed;
    background: transparent;
  }
  .shard.stub.out {
    opacity: 0.28;
    transform: none;
    animation: none;
  }
  .shard.stub .stitle {
    color: var(--faint);
    font-style: italic;
  }
  .led.idle {
    background: var(--led-idle);
    opacity: 0.5;
  }
</style>
