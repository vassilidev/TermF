<script lang="ts">
  import ArrowUpRight from "@lucide/svelte/icons/arrow-up-right";
  import CircleStop from "@lucide/svelte/icons/circle-stop";
  import PanelRightOpen from "@lucide/svelte/icons/panel-right-open";
  import Plus from "@lucide/svelte/icons/plus";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Webhook from "@lucide/svelte/icons/webhook";
  import { lang, t } from "$lib/i18n";
  import { fixed } from "$lib/keymap.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Card from "$lib/ui/Card.svelte";
  import Checklist from "$lib/ui/Checklist.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Field from "$lib/ui/Field.svelte";
  import Hint from "$lib/ui/Hint.svelte";
  import List from "$lib/ui/List.svelte";
  import NextStep from "$lib/ui/NextStep.svelte";
  import Row from "$lib/ui/Row.svelte";
  import Segments from "$lib/ui/Segments.svelte";
  import SidePanel from "$lib/ui/SidePanel.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import StatusPill, { type Status } from "$lib/ui/StatusPill.svelte";
  import StreamSteps from "$lib/ui/StreamSteps.svelte";
  import Switch from "$lib/ui/Switch.svelte";
  import Table from "$lib/ui/Table.svelte";
  import Tabs from "$lib/ui/Tabs.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import WaitCard from "$lib/ui/WaitCard.svelte";
  import Wizard from "$lib/ui/Wizard.svelte";
  import { toast } from "$lib/ui/toast.svelte";

  // The reference page of the design system (palette: "Components (dev)"): every component and its states, for the
  // screenshots and for the slices that rebuild the screens on them. Its data is made up.

  const minutes = new Intl.NumberFormat(lang, { style: "unit", unit: "minute" }).format;
  const hours = new Intl.NumberFormat(lang, { style: "unit", unit: "hour" }).format;
  const money = new Intl.NumberFormat(lang, { style: "currency", currency: "USD" }).format;
  const enter = fixed("Enter");

  const tokens = ["bg", "chrome", "panel", "well", "border", "text", "soft", "muted", "accent", "working", "waiting", "done", "danger"];
  const families = ["cto", "product", "build", "quality", "creative", "specialists"];

  let branch = $state("feat/57-export-csv");
  let wrong = $state("feat/57 export");
  let only = $state(true);
  let filter = $state("all");
  let tab = $state("runs");
  let picked = $state<string | null>("front");
  let sideOpen = $state(true);
  let confirming = $state(false);
  let step = $state(1);
  let started = $state(false);

  type Agent = { role: string; task: string; repo: string; age: string; status: Status };
  const agents: Agent[] = [
    { role: "back", task: t("uiF1RowExport"), repo: "billing-service", age: minutes(6), status: "working" },
    { role: "front", task: t("uiF1RowButton"), repo: "web-dashboard", age: minutes(12), status: "waiting" },
    { role: "qa", task: t("uiF1RowNotes"), repo: "api", age: hours(1), status: "done" },
  ];

  type RunRow = { id: string; title: string; status: Status; time: string; cost: string };
  const runs: RunRow[] = [
    { id: "57", title: "#57 Export CSV", status: "running", time: minutes(25), cost: money(1.84) },
    { id: "479", title: `#479 ${t("uiF1RowButton")}`, status: "delivered", time: t("ciHours", { h: 1, m: 12 }), cost: money(4.1) },
    { id: "481", title: `#481 ${t("uiF1RowNotes")}`, status: "failed", time: minutes(8), cost: money(0.36) },
  ];
</script>

<div class="components">
  <ViewHeader title={t("uiF1Title")} context={t("uiF1Intro")} />
  <div class="scroll">
    <div class="page">
      <section>
        <h2>{t("uiF1Type")}</h2>
        <p class="rule">{t("uiF1TypeRule")}</p>
        <div class="type">
          <span class="meta">28 / 600</span><p class="t-2xl">{t("uiF1TypeDisplay")}</p>
          <span class="meta">20 / 600</span><p class="t-xl">{t("uiF1TypeXl")}</p>
          <span class="meta">16 / 600</span><p class="t-lg">{t("uiF1TypeLg")}</p>
          <span class="meta">14 / 600</span><p class="t-md">{t("uiF1TypeMd")}</p>
          <span class="meta">13 / 400</span><p>{t("uiF1TypeBase")}</p>
          <span class="meta">12 / 400</span><p class="t-sm">{t("uiF1TypeSm")}</p>
          <span class="meta">12 mono</span><p class="t-mono">pytest tests/test_export.py -q</p>
        </div>
        <div class="figures" data-testid="tabular">
          {#each ["12px", "13px"] as size (size)}
            <div class="num" style:font-size={size}>
              <span class="meta">{t("uiF1Tabular")} · {size}</span>
              <span>{money(1111.11)}</span>
              <span>{money(8888.88)}</span>
              <span>{money(470.05)}</span>
            </div>
          {/each}
        </div>
      </section>

      <section>
        <h2>{t("uiF1Colors")}</h2>
        <p class="rule">{t("uiF1ColorsRule")}</p>
        <div class="swatches">
          {#each tokens as token (token)}
            <div class="swatch"><span style:background="var(--{token})"></span><code>--{token}</code></div>
          {/each}
          {#each families as family (family)}
            <div class="swatch"><span style:background="var(--c-{family})"></span><code>--c-{family}</code></div>
          {/each}
        </div>
      </section>

      <section>
        <h2>{t("uiF1Buttons")}</h2>
        <p class="rule">{t("uiF1ButtonsRule")}</p>
        <div class="row">
          <Button variant="primary" kbd={enter}>{t("uiF1Continue")}</Button>
          <Button>{t("uiF1Secondary")}</Button>
          <Button variant="quiet">{t("uiF1Quiet")}</Button>
          <Button variant="danger">{t("delete")}</Button>
          <Button variant="quiet-danger">{t("delete")}</Button>
          <Button variant="primary" loading>{t("uiF1Sending")}</Button>
          <Button disabled>{t("uiF1Disabled")}</Button>
          <Button variant="link">{t("uiF1HintLink")}</Button>
        </div>
        <div class="row">
          <Button size="sm" icon={Plus}>{t("uiF1EmptyAction")}</Button>
          <Button size="sm" variant="quiet" icon={Sparkles} kbd={fixed("Mod+I")}>{t("uiF1AskAi")}</Button>
          <Button size="lg" variant="primary" kbd={enter}>{t("uiF1Continue")}</Button>
          <Button size="lg">{t("uiF1Back")}</Button>
        </div>
      </section>

      <section>
        <h2>{t("uiF1Fields")}</h2>
        <p class="rule">{t("uiF1FieldsRule")}</p>
        <div class="grid two">
          <Field label={t("uiF1BranchLabel")} help={t("uiF1BranchHelp")} example="feat/57-export-csv" bind:value={branch} />
          <Field label={t("uiF1BranchLabel")} error={t("uiF1BranchError")} bind:value={wrong} />
        </div>
      </section>

      <section>
        <h2>{t("uiF1States")}</h2>
        <p class="rule">{t("uiF1StatesRule")}</p>
        <div class="row">
          <StatusPill status="working" />
          <StatusPill status="waiting" />
          <StatusPill status="done" />
          <StatusPill status="failed" />
          <StatusPill status="starting" />
          <StatusPill status="idle" />
        </div>
        <div class="row">
          <Avatar role="back" status="working" label="back" />
          <Avatar role="front" status="waiting" label="front" />
          <Avatar role="qa" status="done" label="qa" />
          <Avatar role="cto" label="cto" />
          <Avatar role="dba" status="failed" label="dba" />
          <Avatar role="ux" label="ux" />
          <Avatar role="brand" label="brand" />
          <Avatar role="lead" size="lg" status="working" label="lead" />
          <Avatar role="po" size="sm" label="po" />
        </div>
      </section>

      <section>
        <h2>{t("uiF1Toggles")}</h2>
        <p class="rule">{t("uiF1TogglesRule")}</p>
        <div class="grid two">
          <div class="stack">
            <Switch bind:checked={only} label={t("uiF1SwitchLabel")} hint={t("uiF1SwitchHint")} />
            <Switch checked={false} label={t("uiF1SwitchOff")} />
            <Switch checked disabled label={t("uiF1Disabled")} />
          </div>
          <div class="stack">
            <Segments
              label={t("uiF1Toggles")}
              options={[
                { id: "all", label: t("uiF1SegAll"), count: 41 },
                { id: "skills", label: t("uiF1SegSkills"), count: 22 },
                { id: "roles", label: t("uiF1SegRoles"), count: 13 },
              ]}
              selected={filter}
              onselect={(id) => (filter = id)}
            />
          </div>
        </div>
        <div class="tabs-demo">
          <Tabs
            id="ui-f1-tabs"
            label={t("uiF1Toggles")}
            tabs={[
              { id: "runs", label: t("uiF1TabRuns") },
              { id: "deploys", label: t("uiF1TabDeploys"), count: 2 },
              { id: "hooks", label: t("uiF1TabHooks"), count: 3 },
            ]}
            selected={tab}
            onselect={(id) => (tab = id)}
          >
            <p class="tab-body muted">{t("uiF1TabBody")}</p>
          </Tabs>
        </div>
      </section>

      <section>
        <h2>{t("uiF1Lists")}</h2>
        <p class="rule">{t("uiF1ListsRule")}</p>
        <List
          label={t("uiF1Lists")}
          items={agents}
          key={(agent) => agent.role}
          selected={picked}
          onselect={(agent) => (picked = agent.role)}
          onopen={(agent) => toast(t("uiF1Opened", { name: agent.role }), { kind: "info" })}
          menu={(agent) => [
            { label: t("uiF1MenuOpen"), icon: ArrowUpRight, kbd: enter, run: () => toast(t("uiF1Opened", { name: agent.role }), { kind: "info" }) },
            { label: t("uiF1AskAi"), icon: Sparkles, kbd: fixed("Mod+I"), run: () => (sideOpen = true) },
            { label: t("uiF1MenuStop"), icon: CircleStop, danger: true, run: () => (confirming = true) },
          ]}
        >
          {#snippet row(agent)}
            <Row title="{agent.role} · {agent.task}" sub="{agent.repo} · {agent.age}">
              {#snippet lead()}<Avatar role={agent.role} status={agent.status} />{/snippet}
              {#snippet trail()}
                <StatusPill status={agent.status} />
                <button
                  class="icon-btn sm"
                  data-testid="ui-row-ask"
                  aria-label={t("uiF1AskAi")}
                  title={t("uiF1AskAi")}
                  onclick={() => (sideOpen = true)}><Sparkles size={14} aria-hidden="true" /></button
                >
              {/snippet}
            </Row>
          {/snippet}
        </List>
      </section>

      <section>
        <h2>{t("uiF1Cards")}</h2>
        <p class="rule">{t("uiF1CardsRule")}</p>
        <div class="grid two">
          <Card title={t("uiF1CardTitle")} count={3}>
            {#snippet actions()}<Button size="sm" variant="quiet">{t("uiF1MenuOpen")}</Button>{/snippet}
            <p class="flat">{t("uiF1CardBody")}</p>
          </Card>
          <Card raised>
            <p class="flat">{t("uiF1CardRaised")}</p>
          </Card>
        </div>
        <Card flush>
          <Table
            label={t("uiF1Cards")}
            columns={[
              { id: "run", label: t("uiF1ColRun") },
              { id: "state", label: t("uiF1ColState") },
              { id: "time", label: t("uiF1ColTime"), numeric: true, width: "120px" },
              { id: "cost", label: t("uiF1ColCost"), numeric: true, width: "120px" },
            ]}
            rows={runs}
            key={(run) => run.id}
            onrow={(run) => toast(t("uiF1Opened", { name: run.title }), { kind: "info" })}
          >
            {#snippet cell(run, column)}
              {#if column.id === "run"}{run.title}{:else if column.id === "state"}<StatusPill status={run.status} />{:else if column.id === "time"}{run.time}{:else}{run.cost}{/if}
            {/snippet}
          </Table>
        </Card>
      </section>

      <section>
        <h2>{t("uiF1Side")}</h2>
        <p class="rule">{t("uiF1SideRule")}</p>
        <div class="side-demo">
          <div class="side-main">
            {#if !sideOpen}
              <Button icon={PanelRightOpen} data-testid="ui-side" onclick={() => (sideOpen = true)}>{t("uiF1SideOpen")}</Button>
            {:else}
              <p class="muted">{t("uiF1SideMain")}</p>
            {/if}
          </div>
          <SidePanel id="ui-f1-demo" title={t("uiF1AskAi")} bind:open={sideOpen}>
            <p class="flat">{t("uiF1SideBody")}</p>
            <StreamSteps
              steps={[
                { verb: t("uiF1StepRead"), object: "src/export/csv.ts", at: "21:52", state: "ok" },
                { verb: t("uiF1StepRan"), object: "pytest -q", at: "21:56", state: "now" },
              ]}
              live
            />
          </SidePanel>
        </div>
      </section>

      <section>
        <h2>{t("uiF1Dialogs")}</h2>
        <p class="rule">{t("uiF1DialogsRule")}</p>
        <div class="row">
          <Button
            data-testid="ui-toast"
            onclick={() => toast(t("uiF1ToastText"), { undo: () => toast(t("uiF1ToastUndone"), { kind: "info" }) })}
            >{t("uiF1ToastShow")}</Button
          >
          <Button onclick={() => toast(t("uiF1ToastError"), { kind: "error" })}>{t("uiF1ToastShowError")}</Button>
          <Button variant="quiet-danger" data-testid="ui-dialog" onclick={() => (confirming = true)}>{t("uiF1DialogOpen")}</Button>
        </div>
        <div class="toast-card toast-still" aria-hidden="true">
          <span>{t("uiF1ToastText")}</span>
          <span class="btn sm">{t("uiF1Undo")}<kbd>{fixed("Mod+Z")}</kbd></span>
        </div>
        <ConfirmDialog
          bind:open={confirming}
          title={t("uiF1DialogTitle")}
          confirm={t("uiF1DialogConfirm")}
          cancel={t("uiF1DialogKeep")}
          danger
          onconfirm={() => toast(t("uiF1DialogDone"), { kind: "info" })}
        >
          <p>{t("uiF1DialogBody")}</p>
        </ConfirmDialog>
      </section>

      <section>
        <h2>{t("uiF1Skeletons")}</h2>
        <p class="rule">{t("uiF1SkeletonsRule")}</p>
        <div class="grid two">
          <div class="frame"><Skeleton shape="list" rows={3} delay={0} /></div>
          <div class="frame"><Skeleton shape="lines" rows={4} delay={0} /></div>
          <div class="frame"><Skeleton shape="table" rows={4} delay={0} /></div>
          <div class="frame"><Skeleton shape="form" rows={3} delay={0} /></div>
        </div>
        <Skeleton shape="cards" rows={3} delay={0} />
      </section>

      <section>
        <h2>{t("uiF1Streams")}</h2>
        <p class="rule">{t("uiF1StreamsRule")}</p>
        <div class="grid two">
          <div class="frame">
            <StreamSteps
              live
              steps={[
                { verb: t("uiF1StepRead"), object: "src/export/csv.ts", at: "21:52", state: "ok" },
                { verb: t("uiF1StepFailed"), object: "test_rounding", at: "21:56", state: "fail" },
                { verb: t("uiF1StepFix"), object: "money.py", at: "21:57", state: "now" },
              ]}
            />
          </div>
          <div class="frame">
            <StreamSteps
              summary={t("uiF1StreamSummary")}
              steps={[
                { verb: t("uiF1StepRead"), object: "src/export/csv.ts", at: "21:52" },
                { verb: t("uiF1StepRead"), object: "src/money.py", at: "21:52" },
                { verb: t("uiF1StepRead"), object: "tests/test_export.py", at: "21:53" },
                { verb: t("uiF1StepSearched"), object: "round(", at: "21:53" },
                { verb: t("uiF1StepRan"), object: "pytest -q", at: "21:54", state: "ok" },
              ]}
            />
          </div>
        </div>
      </section>

      <section>
        <h2>{t("uiF1Wizards")}</h2>
        <p class="rule">{t("uiF1WizardsRule")}</p>
        <div class="wizard-demo">
          <Wizard
            title={t("uiF1Wizards")}
            steps={[
              { id: "repos", label: t("uiF1WizRepos"), answer: "api, web-dashboard, mobile" },
              { id: "understood", label: t("uiF1WizUnderstood"), answer: t("uiF1WizUnderstoodAnswer") },
              { id: "rules", label: t("uiF1WizRules") },
              { id: "check", label: t("uiF1WizCheck") },
            ]}
            current={step}
            onstep={(i) => (step = i)}
            onback={() => (step = Math.max(0, step - 1))}
            oncontinue={() => (step = Math.min(3, step + 1))}
          >
            <h3 class="t-xl">{t("uiF1WizQuestion")}</h3>
            <p class="muted">{t("uiF1WizQuestionHelp")}</p>
            <Segments
              label={t("uiF1WizQuestion")}
              options={[
                { id: "pr", label: t("uiF1WizPr") },
                { id: "branch", label: t("uiF1WizBranch") },
              ]}
              selected="pr"
              onselect={() => {}}
            />
            {#snippet aside()}
              <Hint text={t("uiF1HintText")} proof={t("uiF1HintProof")} />
            {/snippet}
          </Wizard>
        </div>
      </section>

      <section>
        <h2>{t("uiF1Empties")}</h2>
        <p class="rule">{t("uiF1EmptiesRule")}</p>
        <div class="frame">
          <EmptyState
            icon={Webhook}
            title={t("uiF1EmptyTitle")}
            text={t("uiF1EmptyText")}
            action={{ label: t("uiF1EmptyAction"), icon: Plus, run: () => toast(t("uiF1EmptyAction"), { kind: "info" }) }}
            onask={() => (sideOpen = true)}
            askLabel={t("uiF1EmptyAsk")}
          />
        </div>
      </section>

      <section>
        <h2>{t("uiF1Guides")}</h2>
        <p class="rule">{t("uiF1GuidesRule")}</p>
        <div class="stack">
          <WaitCard title={t("uiF1WaitTitle")} meta="{t("uiF1ColRun")} #57 · {minutes(12)}" action={{ label: t("uiF1WaitAnswer"), kbd: fixed("Mod+J"), run: () => {} }} />
          <NextStep text={t("uiF1NextText")} action={{ label: t("uiF1NextAction"), run: () => {} }} />
          <div class="grid two">
            <Checklist
              title={t("uiF1ChecklistTitle")}
              items={[
                { id: "claude", label: t("uiF1CheckClaude"), done: true },
                { id: "project", label: t("uiF1CheckProject"), done: true },
                { id: "tickets", label: t("uiF1CheckTickets"), done: started, run: () => (started = true), action: t("uiF1CheckAction") },
                { id: "run", label: t("uiF1CheckRun"), done: false },
                { id: "review", label: t("uiF1CheckReview"), done: false },
              ]}
            />
            <div class="stack">
              <Hint text={t("uiF1HintText")} proof={t("uiF1HintProof")} link={{ label: t("uiF1HintLink"), run: () => {} }} />
            </div>
          </div>
        </div>
      </section>
    </div>
  </div>
</div>

<style>
  .components {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg);
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .page {
    display: flex;
    flex-direction: column;
    gap: 48px;
    max-width: 1040px;
    margin: 0 auto;
    padding: 32px 32px 64px;
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .rule {
    max-width: 70ch;
    margin: -8px 0 0;
    color: var(--muted);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }

  .grid {
    display: grid;
    gap: 16px;
  }

  .grid.two {
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .frame {
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
  }

  .flat {
    margin: 0;
  }

  .type {
    display: grid;
    grid-template-columns: 96px 1fr;
    align-items: baseline;
    gap: 10px 16px;
  }

  .type p {
    margin: 0;
  }

  .meta {
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  .t-2xl {
    font-size: var(--text-2xl);
    font-weight: 600;
    letter-spacing: -0.02em;
    line-height: 1.15;
  }

  .t-xl {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.015em;
    line-height: 1.25;
  }

  .t-lg {
    font-size: var(--text-lg);
    font-weight: 600;
  }

  .t-md {
    font-size: var(--text-md);
    font-weight: 600;
  }

  .t-sm {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .t-mono {
    font: var(--text-sm) var(--font-mono);
  }

  .figures {
    display: flex;
    gap: 32px;
  }

  .figures .num {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
  }

  .swatches {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 10px;
  }

  .swatch {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .swatch span {
    width: 28px;
    height: 28px;
    flex-shrink: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .swatch code {
    color: var(--soft);
    font: var(--text-xs) var(--font-mono);
  }

  .tabs-demo {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }

  .tab-body {
    margin: 0;
    padding: 16px;
  }

  .side-demo {
    position: relative;
    display: flex;
    height: 280px;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }

  .side-main {
    display: grid;
    flex: 1;
    place-items: center;
    min-width: 0;
    padding: 16px;
  }

  .wizard-demo {
    height: 380px;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg);
  }

  .wizard-demo :global(.content) {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
  }

  .toast-still {
    align-self: flex-start;
  }
</style>
