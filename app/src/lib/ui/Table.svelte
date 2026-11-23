<script lang="ts" module>
  /** `numeric`: aligned right, tabular figures. */
  export type Column = { id: string; label: string; numeric?: boolean; width?: string };
</script>

<script lang="ts" generics="T">
  import type { Snippet } from "svelte";

  /**
   * Columns and rows; a discreet header, numbers on the right. With `onrow`, the whole row opens its item
   * (click, or Enter on the row's focus); the cells come from `cell(row, column)`.
   */
  let {
    label,
    columns,
    rows,
    key,
    cell,
    onrow,
  }: {
    label: string;
    columns: Column[];
    rows: T[];
    key: (row: T) => string;
    cell: Snippet<[T, Column]>;
    onrow?: (row: T) => void;
  } = $props();
</script>

<table class="table" aria-label={label}>
  <thead>
    <tr>
      {#each columns as column (column.id)}
        <th scope="col" class:num={column.numeric} style:width={column.width}>{column.label}</th>
      {/each}
    </tr>
  </thead>
  <tbody>
    {#each rows as row (key(row))}
      <tr
        class:open={!!onrow}
        tabindex={onrow ? 0 : undefined}
        onclick={() => onrow?.(row)}
        onkeydown={(e) => {
          if (onrow && e.key === "Enter") onrow(row);
        }}
      >
        {#each columns as column (column.id)}
          <td class:num={column.numeric}>{@render cell(row, column)}</td>
        {/each}
      </tr>
    {/each}
  </tbody>
</table>

<style>
  .table {
    width: 100%;
    border-collapse: collapse;
  }

  th {
    height: 32px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 500;
    text-align: left;
  }

  td {
    height: var(--row);
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
  }

  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  tr.open {
    cursor: pointer;
  }

  tbody tr:hover td {
    background: var(--hover);
  }

  tr:focus-visible {
    outline-offset: -2px;
  }
</style>
