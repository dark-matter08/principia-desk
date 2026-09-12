<script lang="ts">
  import type { CurriculumBrief } from '../ipc';
  import { ArrowRight, Boxes, Link2, Target } from 'lucide-svelte';

  let {
    whyNow,
    curriculum,
    prerequisites,
  }: {
    whyNow: string;
    curriculum: CurriculumBrief;
    prerequisites: string[];
  } = $props();
</script>

<aside class="course-purpose" aria-labelledby="course-purpose-title">
  <div class="purpose-main">
    <span class="eyebrow mono">{curriculum.phase} · {curriculum.core ? 'core path' : 'elective'}</span>
    <h2 id="course-purpose-title">Why this course now</h2>
    <p>{whyNow}</p>
  </div>
  <div class="purpose-cell">
    <span class="cell-title mono"><Target size={11} /> OUTCOME</span>
    <p>{curriculum.learner_outcome}</p>
  </div>
  <div class="purpose-cell">
    <span class="cell-title mono"><Boxes size={11} /> PROOF-OF-SKILL</span>
    <p>{curriculum.artifact}</p>
  </div>
  <div class="purpose-cell">
    <span class="cell-title mono"><Link2 size={11} /> PREREQUISITES</span>
    {#if prerequisites.length}
      <p>{prerequisites.join(' · ')}</p>
    {:else}
      <p>Entry point: no prior concept is assumed.</p>
    {/if}
    <span class="advance mono">exercise <ArrowRight size={10} /> portfolio evidence</span>
  </div>
</aside>

<style>
  .course-purpose {
    margin: 0 18px 12px;
    border: 1px solid rgba(62, 207, 142, 0.24);
    border-radius: var(--radius-panel);
    background: rgba(8, 18, 31, 0.9);
    display: grid;
    grid-template-columns: minmax(240px, 1.35fr) repeat(3, minmax(170px, 1fr));
    align-items: stretch;
  }
  /* Every cell shares one rhythm: eyebrow on the same line across the strip,
     then the text, with the same padding on every side. */
  .purpose-main,
  .purpose-cell {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 16px 18px 18px;
    min-width: 0;
  }
  .purpose-cell {
    border-left: 1px solid rgba(139, 159, 184, 0.14);
  }
  .eyebrow,
  .cell-title,
  .advance {
    font-size: 8px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    line-height: 14px;
  }
  .eyebrow,
  .cell-title {
    color: #3ecf8e;
  }
  .eyebrow,
  .cell-title,
  .advance {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  h2 {
    margin: 0;
    color: #e7eff8;
    font-size: 15px;
    line-height: 1.25;
  }
  p {
    margin: 0;
    color: #94a6ba;
    font-size: 10.5px;
    line-height: 1.55;
  }
  .advance {
    margin-top: auto;
    padding-top: 8px;
    color: #6f849b;
  }
  @media (max-width: 980px) {
    .course-purpose {
      grid-template-columns: 1fr 1fr;
    }
    .purpose-cell:nth-child(3) {
      border-left: 0;
      border-top: 1px solid rgba(139, 159, 184, 0.14);
    }
    .purpose-cell:nth-child(4) {
      border-top: 1px solid rgba(139, 159, 184, 0.14);
    }
  }
</style>
