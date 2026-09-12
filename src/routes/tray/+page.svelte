<script lang="ts">
  import '../../lib/theme.css';
  import TrayPanel from '../../lib/features/tray/TrayPanel.svelte';
</script>

<svelte:head>
  <style>
    html, body { background: transparent !important; overflow: hidden !important; }
  </style>
</svelte:head>

<div class="theme-noir tray-root"><TrayPanel /></div>

<style>
  .tray-root { background: transparent; }
</style>
