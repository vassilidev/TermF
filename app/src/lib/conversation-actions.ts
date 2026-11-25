import type { Action } from "$lib/actions.svelte";
import { agentProjects, agents, deleteConversations, newTopic } from "$lib/agents.svelte";
import { t } from "$lib/i18n";
import { toast } from "$lib/ui/toast.svelte";

const busy = (id: number) => ["working", "waiting"].includes(agents.sessions.find((a) => a.id === id)?.state ?? "");

/** Its turn is under way while it calls: the topic changes once the turn ends, so the agent reads the answer first. */
function afterTurn(id: number, then: () => void) {
  const tick = setInterval(() => {
    if (busy(id)) return;
    clearInterval(tick);
    then();
  }, 500);
}

const topic: Action = {
  id: "conversation.new_topic",
  description:
    "Topic done, as the user's button says it: once your turn ends, this conversation is archived (kept, out of the list) and an empty one with the same role takes its place. Project memory and decisions stay; this thread does not. Use it when the user wants to start over on another subject.",
  fields: {},
  label: "topicActNew",
  describe: () => t("topicActNewText"),
  async run(_args, _secrets, _project, session) {
    if (!agents.sessions.some((a) => a.id === session)) throw new Error(`no conversation ${session}`);
    afterTurn(session, () => {
      const now = agents.sessions.find((a) => a.id === session);
      if (now && !now.archived) void newTopic(now);
    });
    return { archived: session, when: "at the end of this turn" };
  },
};

/** The project's archived conversations, the ones named or all. */
function archivedOf(args: Record<string, unknown>, project: string) {
  const ids = Array.isArray(args.conversations) ? args.conversations.map(Number) : null;
  return agents.sessions.filter((a) => a.archived && agentProjects(a).includes(project) && (ids === null || ids.includes(a.id)));
}

const remove: Action = {
  id: "conversation.delete",
  description:
    "Deletes archived conversations of the project for good, their threads and stored events with them. Only archived ones (a topic done); all of them when `conversations` is left out.",
  fields: {
    conversations: {
      type: "array",
      description: 'The ids of the archived conversations to delete, as texts ("12"); all of them when left out.',
    },
  },
  label: "topicActDelete",
  describe: (args, project) => {
    const list = archivedOf(args, project);
    return t("topicActDeleteText", { count: list.length, titles: list.map((a) => a.title).join(", ") });
  },
  loses: () => t("topicActDeleteLoses"),
  async run(args, _secrets, project) {
    const list = archivedOf(args, project);
    if (!list.length) throw new Error("no archived conversation to delete");
    await deleteConversations(list.map((a) => a.id));
    toast(t("topicDeleted", { count: list.length }));
    return { deleted: list.map((a) => a.id) };
  },
};

export const conversationActions: Action[] = [topic, remove];
