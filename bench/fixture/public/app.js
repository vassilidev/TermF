const list = document.querySelector("#tasks");
const form = document.querySelector("#new-task");
const title = document.querySelector("#title");
const filter = document.querySelector("#filter");
const error = document.querySelector("#error");

async function api(path, options = {}) {
  const res = await fetch(path, { headers: { "content-type": "application/json" }, ...options });
  const body = res.status === 204 ? null : await res.json();
  if (!res.ok) throw new Error(body?.error ?? res.statusText);
  return body;
}

function show(err) {
  error.textContent = err.message;
}

async function render() {
  const query = filter.value ? `?done=${filter.value}` : "";
  const tasks = await api(`/api/tasks${query}`);
  error.textContent = "";
  list.replaceChildren(...tasks.map(taskItem));
}

function taskItem(task) {
  const item = document.createElement("li");
  const box = document.createElement("input");
  box.type = "checkbox";
  box.checked = task.done;
  box.setAttribute("aria-label", `Done: ${task.title}`);
  box.addEventListener("change", () => {
    api(`/api/tasks/${task.id}`, { method: "PATCH", body: JSON.stringify({ done: box.checked }) }).then(render, show);
  });
  const label = document.createElement("span");
  label.textContent = task.title;
  const remove = document.createElement("button");
  remove.textContent = "Delete";
  remove.addEventListener("click", () => {
    api(`/api/tasks/${task.id}`, { method: "DELETE" }).then(render, show);
  });
  item.append(box, label, remove);
  return item;
}

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  await api("/api/tasks", { method: "POST", body: JSON.stringify({ title: title.value }) });
  form.reset();
  await render();
});

filter.addEventListener("change", () => render().catch(show));

render().catch(show);
