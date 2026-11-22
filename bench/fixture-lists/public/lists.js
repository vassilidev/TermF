import { api, element, showError } from "./api.js";

const nav = document.querySelector("#lists");
const form = document.querySelector("#new-list");
const nameInput = document.querySelector("#list-name");

let selectedId = null;
let onSelect = () => {};

/** `select(list)` is called with the chosen list, or null when none is left. */
export function initLists(select) {
  onSelect = select;
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    try {
      const list = await api("/api/lists", { method: "POST", body: { name: nameInput.value } });
      form.reset();
      selectedId = list.id;
      await renderLists();
    } catch (err) {
      showError(err);
    }
  });
}

export async function renderLists() {
  const lists = await api("/api/lists");
  if (!lists.some((list) => list.id === selectedId)) selectedId = lists[0]?.id ?? null;
  nav.replaceChildren(...lists.map(listItem));
  onSelect(lists.find((list) => list.id === selectedId) ?? null);
}

function listItem(list) {
  const button = element(
    "button",
    { type: "button", title: `${list.open_count} open of ${list.task_count}` },
    element("span", { textContent: list.name }),
    element("span", { className: "count", textContent: String(list.open_count) }),
  );
  button.setAttribute("aria-current", String(list.id === selectedId));
  button.addEventListener("click", () => {
    selectedId = list.id;
    renderLists().catch(showError);
  });
  return element("li", {}, button);
}

export async function renameList(list) {
  const name = prompt("New name", list.name);
  if (name === null) return;
  await api(`/api/lists/${list.id}`, { method: "PATCH", body: { name } });
  await renderLists();
}

export async function deleteList(list) {
  if (!confirm(`Delete "${list.name}" and its ${list.task_count} tasks?`)) return;
  await api(`/api/lists/${list.id}`, { method: "DELETE" });
  selectedId = null;
  await renderLists();
}
