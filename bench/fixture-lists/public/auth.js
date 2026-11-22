import { api, session, showError } from "./api.js";

const form = document.querySelector("#signin-form");

export function initAuth(onSignedIn) {
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const { username, password, email } = Object.fromEntries(new FormData(form));
    try {
      if (event.submitter?.value === "up") {
        await api("/api/users", { method: "POST", body: { username, password, email: email || null } });
      }
      const { token } = await api("/api/sessions", { method: "POST", body: { username, password } });
      session.set(token);
      form.reset();
      showError(null);
      onSignedIn();
    } catch (err) {
      showError(err);
    }
  });

  document.querySelector("#sign-out").addEventListener("click", async () => {
    await api("/api/sessions", { method: "DELETE" }).catch(() => {});
    session.set(null);
    dispatchEvent(new Event("signed-out"));
  });
}
