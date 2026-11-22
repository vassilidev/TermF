const TOKEN_KEY = "lists.token";

export const session = {
  get token() {
    return localStorage.getItem(TOKEN_KEY);
  },
  set(token) {
    if (token) localStorage.setItem(TOKEN_KEY, token);
    else localStorage.removeItem(TOKEN_KEY);
  },
};

/** Calls the API; answers the parsed body, or throws an Error carrying the API's error message. */
export async function api(path, { method = "GET", body } = {}) {
  const headers = {};
  if (body !== undefined) headers["content-type"] = "application/json";
  if (session.token) headers.authorization = `Bearer ${session.token}`;
  const res = await fetch(path, { method, headers, body: body === undefined ? undefined : JSON.stringify(body) });
  const data = res.status === 204 ? null : await res.json();
  if (res.status === 401 && session.token) {
    session.set(null);
    dispatchEvent(new Event("signed-out"));
  }
  if (!res.ok) throw new Error(data?.error ?? res.statusText);
  return data;
}

const errorBox = document.querySelector("#error");

export function showError(err) {
  errorBox.textContent = err ? err.message : "";
}

export function element(tag, props = {}, ...children) {
  const node = document.createElement(tag);
  Object.assign(node, props);
  node.append(...children);
  return node;
}
