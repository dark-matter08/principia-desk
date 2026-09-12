<script lang="ts">
  import '../../lib/theme.css';
  import RecoveryConsole from '../../lib/features/recovery/RecoveryConsole.svelte';
</script>

<svelte:head>
  <style>
    html, body { margin: 0; height: 100%; overflow: hidden; }
  </style>
</svelte:head>

<div class="theme-noir recovery-root"><RecoveryConsole /></div>

<style>
  .recovery-root { height: 100vh; background: var(--bg); }
</style>
