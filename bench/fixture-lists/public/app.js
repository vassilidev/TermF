import { api, session, showError } from "./api.js";
import { initAuth } from "./auth.js";
import { initLists, renderLists } from "./lists.js";
import { initTasks, showList } from "./tasks.js";

const signin = document.querySelector("#signin");
const app = document.querySelector("#app");
const account = document.querySelector("#account");
const whoami = document.querySelector("#whoami");

function showSignIn() {
  signin.hidden = false;
  app.hidden = true;
  account.hidden = true;
}

async function start() {
  if (!session.token) return showSignIn();
  try {
    const me = await api("/api/me");
    whoami.textContent = me.username;
    signin.hidden = true;
    app.hidden = false;
    account.hidden = false;
    await renderLists();
  } catch (err) {
    showError(err);
    showSignIn();
  }
}

initAuth(start);
initLists((list) => showList(list).catch(showError));
initTasks();
addEventListener("signed-out", showSignIn);
start();
