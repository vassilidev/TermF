<script lang="ts">
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import { t } from "$lib/i18n";

  /** The card of a change on a server: what it loses, then the environment's name typed back, as a recipe that writes asks. */
  let {
    open = $bindable(false),
    title,
    loses,
    env,
    onconfirm,
  }: { open?: boolean; title: string; loses: string; env: string; onconfirm: (typed: string) => void } = $props();

  let typed = $state("");
  $effect(() => {
    if (open) typed = "";
  });
</script>

<ConfirmDialog bind:open {title} confirm={title} danger disabled={typed.trim() !== env} onconfirm={() => onconfirm(typed.trim())}>
  <p><b>{t("opsmoreLoses")}</b> · {loses}</p>
  <p>{t("opsConfirmBody", { env })}</p>
  <label class="labelled">
    <span>{t("opsConfirmField")}</span>
    <input class="field mono" bind:value={typed} placeholder={env} />
  </label>
</ConfirmDialog>
