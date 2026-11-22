import { api, element, showError } from "./api.js";
import { deleteList, renameList, renderLists } from "./lists.js";

const view = document.querySelector("#list-view");
const heading = document.querySelector("#list-title");
const items = document.querySelector("#tasks");
const form = document.querySelector("#new-task");
const titleInput = document.querySelector("#task-title");
const dueInput = document.querySelector("#task-due");
const priorityInput = document.querySelector("#task-priority");
const doneFilter = document.querySelector("#filter-done");
const tagFilter = document.querySelector("#filter-tag");
const sortSelect = document.querySelector("#sort");
const activityItems = document.querySelector("#activity-items");

const PRIORITIES = ["Low", "Normal", "High", "Urgent"];

let current = null;

export function initTasks() {
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const body = { title: titleInput.value, priority: Number(priorityInput.value), due_date: dueInput.value || null };
    try {
      await api(`/api/lists/${current.id}/tasks`, { method: "POST", body });
      form.reset();
      await refresh();
    } catch (err) {
      showError(err);
    }
  });
  for (const control of [doneFilter, tagFilter, sortSelect]) {
    control.addEventListener("change", () => renderTasks().catch(showError));
  }
  document.querySelector("#rename-list").addEventListener("click", () => renameList(current).catch(showError));
  document.querySelector("#delete-list").addEventListener("click", () => deleteList(current).catch(showError));
}

/** Shows a list, or hides the view when `list` is null. */
export async function showList(list) {
  const changed = list?.id !== current?.id;
  current = list;
  view.hidden = !list;
  if (!list) return;
  heading.textContent = list.name;
  if (changed) tagFilter.value = "";
  await Promise.all([renderTags(), renderTasks(), renderActivity()]);
}

async function refresh() {
  showError(null);
  await renderLists();
}

async function renderTasks() {
  const query = new URLSearchParams({ sort: sortSelect.value });
  if (doneFilter.value) query.set("done", doneFilter.value);
  if (tagFilter.value) query.set("tag", tagFilter.value);
  const tasks = await api(`/api/lists/${current.id}/tasks?${query}`);
  items.replaceChildren(...tasks.map(taskItem));
}

async function renderTags() {
  const tags = await api(`/api/lists/${current.id}/tags`);
  const chosen = tagFilter.value;
  tagFilter.replaceChildren(
    element("option", { value: "", textContent: "Any" }),
    ...tags.map((tag) => element("option", { value: tag.name, textContent: `${tag.name} (${tag.count})` })),
  );
  tagFilter.value = tags.some((tag) => tag.name === chosen) ? chosen : "";
}

async function renderActivity() {
  const entries = await api(`/api/lists/${current.id}/activity?limit=20`);
  activityItems.replaceChildren(
    ...entries.map((entry) =>
      element("li", { textContent: `${new Date(entry.created_at).toLocaleString()}: ${entry.user?.username ?? "someone"} ${describe(entry)}` }),
    ),
  );
}

function describe(entry) {
  switch (entry.action) {
    case "task.created":
      return `added "${entry.detail.title}"`;
    case "task.updated":
      return `changed ${entry.detail.changes.join(", ")} of task #${entry.task_id}`;
    case "task.deleted":
      return `deleted "${entry.detail.title}"`;
    case "comment.added":
      return `commented on task #${entry.task_id}`;
    case "list.renamed":
      return `renamed the list to "${entry.detail.to}"`;
    default:
      return entry.action;
  }
}

function taskItem(task) {
  const box = element("input", { type: "checkbox", checked: task.done });
  box.setAttribute("aria-label", `Done: ${task.title}`);
  box.addEventListener("change", () => patch(task, { done: box.checked }));

  const title = element("span", { className: `task-title priority-${task.priority}`, textContent: task.title });
  title.addEventListener("dblclick", () => {
    const next = prompt("Title", task.title);
    if (next !== null) patch(task, { title: next });
  });

  const tagsButton = element("button", { type: "button", textContent: "Tags" });
  tagsButton.addEventListener("click", () => {
    const next = prompt("Tags, separated by commas", task.tags.join(", "));
    if (next !== null) patch(task, { tags: next.split(",").map((tag) => tag.trim()).filter(Boolean) });
  });

  const commentsButton = element("button", { type: "button", textContent: `Comments (${task.comment_count})` });
  const remove = element("button", { type: "button", textContent: "Delete" });
  remove.addEventListener("click", () => {
    api(`/api/tasks/${task.id}`, { method: "DELETE" }).then(refresh, showError);
  });

  const meta = element(
    "div",
    { className: "meta" },
    element("span", { textContent: PRIORITIES[task.priority] }),
    ...(task.due_date ? [element("span", { textContent: `due ${task.due_date}` })] : []),
    ...(task.assignee ? [element("span", { textContent: `@${task.assignee.username}` })] : []),
    ...task.tags.map((tag) => element("span", { className: "tag", textContent: tag })),
  );
  const item = element(
    "li",
    { className: task.done ? "task done" : "task" },
    element("div", { className: "task-line" }, box, title, tagsButton, commentsButton, remove),
    meta,
  );
  commentsButton.addEventListener("click", () => toggleComments(task, item).catch(showError));
  return item;
}

function patch(task, changes) {
  api(`/api/tasks/${task.id}`, { method: "PATCH", body: changes }).then(refresh, showError);
}

async function toggleComments(task, item) {
  const open = item.querySelector(".comments");
  if (open) return open.remove();
  const comments = await api(`/api/tasks/${task.id}/comments`);
  const input = element("input", { placeholder: "Add a comment", maxLength: 2000 });
  const send = element("form", { className: "row" }, input, element("button", { type: "submit", textContent: "Send" }));
  send.addEventListener("submit", async (event) => {
    event.preventDefault();
    try {
      await api(`/api/tasks/${task.id}/comments`, { method: "POST", body: { body: input.value } });
      await refresh();
    } catch (err) {
      showError(err);
    }
  });
  const list = element(
    "ul",
    {},
    ...comments.map((comment) => element("li", { textContent: `${comment.author?.username ?? "someone"}: ${comment.body}` })),
  );
  item.append(element("div", { className: "comments" }, list, send));
}
