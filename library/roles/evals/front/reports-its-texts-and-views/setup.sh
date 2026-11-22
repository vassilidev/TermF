#!/bin/sh
set -e
mkdir -p web/src/invoices web/src/lib
cat > web/src/lib/i18n.ts <<'EOF'
const fr = {
  invoicesTitle: "Factures",
  invoicesNumber: "Numéro",
  invoicesAmount: "Montant",
};

const en: Record<keyof typeof fr, string> = {
  invoicesTitle: "Invoices",
  invoicesNumber: "Number",
  invoicesAmount: "Amount",
};

export const t = (key: keyof typeof fr) => (navigator.language.startsWith("fr") ? fr : en)[key];
EOF
cat > web/src/invoices/List.svelte <<'EOF'
<script lang="ts">
  import { t } from "$lib/i18n";
  let { invoices }: { invoices: { id: string; number: string; amount: string }[] } = $props();
</script>

<h1>{t("invoicesTitle")}</h1>
<table>
  <thead>
    <tr><th>{t("invoicesNumber")}</th><th>{t("invoicesAmount")}</th></tr>
  </thead>
  <tbody>
    {#each invoices as invoice (invoice.id)}
      <tr><td>{invoice.number}</td><td>{invoice.amount}</td></tr>
    {/each}
  </tbody>
</table>
EOF
