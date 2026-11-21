<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import SessionGrid from "$lib/SessionGrid.svelte";
  import Sidebar from "$lib/Sidebar.svelte";
  import { lang } from "$lib/i18n";
  import { init } from "$lib/state.svelte";

  onMount(() => {
    document.documentElement.lang = lang;
    return init();
  });
</script>

<div class="app">
  <Sidebar />
  <SessionGrid />
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 280px 1fr;
    height: 100vh;
  }
</style>
