<script lang="ts">
  import FolderGit2 from "@lucide/svelte/icons/folder-git-2";
  import Globe from "@lucide/svelte/icons/globe";
  import { forge } from "$lib/setup-logic";

  /** The mark of the forge a repository's remote is on; a repository without remote gets a plain folder. */
  let { host, size = 16 }: { host?: string; size?: number } = $props();

  const kind = $derived(forge(host));
</script>

{#if kind === "github"}
  <!-- GitHub's mark, from Octicons (MIT). -->
  <svg width={size} height={size} viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
    <path
      d="M8 0c4.42 0 8 3.58 8 8a8.01 8.01 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A8 8 0 0 1 0 8c0-4.42 3.58-8 8-8Z"
    />
  </svg>
{:else if kind === "gitlab"}
  <!-- GitLab's mark, from Simple Icons (CC0). -->
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
    <path
      d="m23.6 9.59-.03-.09L20.3.98a.85.85 0 0 0-.34-.4.87.87 0 0 0-1 .05.87.87 0 0 0-.29.44l-2.2 6.75H7.54L5.33 1.07a.86.86 0 0 0-.29-.44.87.87 0 0 0-1-.06.86.86 0 0 0-.34.41L.43 9.5l-.03.09a6.07 6.07 0 0 0 2.01 7.01l.01.01.03.02 4.98 3.73 2.46 1.86 1.5 1.13a1 1 0 0 0 1.22 0l1.5-1.13 2.46-1.86 5.01-3.75.01-.01a6.07 6.07 0 0 0 2.01-7z"
    />
  </svg>
{:else if kind === "bitbucket"}
  <!-- Bitbucket's mark, from Simple Icons (CC0). -->
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
    <path
      d="M.78 1.21a.77.77 0 0 0-.77.9l3.26 19.81c.09.5.52.87 1.03.87h15.65a.77.77 0 0 0 .77-.64l3.27-20.03a.77.77 0 0 0-.77-.9zm13.74 14.32H9.52L8.17 8.47h7.56z"
    />
  </svg>
{:else if kind === "other"}
  <Globe size={size} aria-hidden="true" />
{:else}
  <FolderGit2 size={size} aria-hidden="true" />
{/if}
