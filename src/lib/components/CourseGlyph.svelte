<script lang="ts">
  import type { ClassroomSubjectId } from '../catalog.generated';
  let { courseId, size = 42 }: { courseId: ClassroomSubjectId; size?: number } = $props();
</script>

<!-- Small, code-native course marks extend the blueprint vocabulary on main.
     They are decorative: the adjacent course title supplies the accessible name. -->
<svg width={size} height={size} viewBox="0 0 48 48" fill="none" aria-hidden="true" class="course-glyph">
  <rect x="1" y="1" width="46" height="46" rx="6" class="frame" />
  <g stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
    {#if courseId === 'system-design'}
      <path d="M24 16v7M12 30v-7h24v7" class="pipe" />
      <rect x="17" y="8" width="14" height="8" rx="2" /><rect x="6" y="30" width="12" height="9" rx="2" /><rect x="30" y="30" width="12" height="9" rx="2" />
    {:else if courseId === 'javascript'}
      <rect x="8" y="10" width="32" height="28" rx="3" /><path d="M8 18h32M12 14h1m4 0h1M20 24l-5 4 5 4m8-8 5 4-5 4" />
    {:else if courseId === 'typescript'}
      <path d="M24 7 38 13v12c0 8-14 16-14 16S10 33 10 25V13Z" /><path d="m17 24 5 5 10-11" class="signal" />
    {:else if courseId === 'frontend-architecture'}
      <rect x="8" y="8" width="32" height="9" rx="2" /><rect x="8" y="22" width="10" height="18" rx="2" /><rect x="23" y="22" width="17" height="7" rx="2" /><path d="M23 35h17M23 40h11" class="pipe" />
    {:else if courseId === 'developer-tooling'}
      <path d="M14 24h7m6 0h7" class="pipe" /><rect x="5" y="18" width="9" height="12" rx="2" /><path d="m24 17 7 7-7 7-7-7Z" /><rect x="34" y="18" width="9" height="12" rx="2" />
    {:else if courseId === 'linux-bash'}
      <rect x="7" y="9" width="34" height="30" rx="3" /><path d="M7 16h34m-28 7 5 4-5 4m10 0h9" /><circle cx="12" cy="12.5" r="1" class="signal" />
    {:else if courseId === 'bash-scripting'}
      <path d="M14 7h15l8 8v26H14Zm15 0v8h8M10 17H7v20h3M20 22l4 3-4 3m8 0h3" /><path d="M20 34h11" class="pipe" />
    {:else if courseId.startsWith('custom-')}
      <!-- A learner's own class: a sheet they wrote on, with the pen still there. -->
      <path d="M10 8h20l8 8v24H10Zm20 0v8h8" /><path d="M15 22h12M15 28h9" class="pipe" /><path d="m27 35 9-9 3 3-9 9h-3Z" class="signal-stroke" />
    {:else}
      <path d="M7 10h25v19H19l-7 6v-6H7Z" /><path d="M36 18h5v20h-5v5l-7-5h-8v-5" class="pipe" />
      {#if courseId === 'german'}<path d="M13 17h12m-12 6h8" />{:else}<path d="M19 15v10m-5-10h10m-10 10h10" />{/if}
    {/if}
  </g>
  <circle cx="40" cy="8" r="2.5" class="signal" />
</svg>

<style>
  .course-glyph { flex: 0 0 auto; color: var(--accent); }
  .frame { fill: var(--bg); stroke: var(--node-border); }
  .pipe { stroke: var(--violet-fg); stroke-dasharray: 2.5 3; }
  .signal { fill: var(--led-ok); stroke: var(--bg); }
  .signal-stroke { stroke: var(--led-ok); }
</style>
