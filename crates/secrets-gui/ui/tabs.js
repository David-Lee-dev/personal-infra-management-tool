// 왼쪽 메뉴로 화면을 바꾼다. 화면을 처음 열 때만 데이터를 읽는다.

import { loadAccounts } from "./accounts/index.js";
import { loadKeys } from "./keys/index.js";
import { loadProjects } from "./projects/index.js";
import { openJobs } from "./terminal.js";

const sidebar = document.getElementById("sidebar");
const panels = {
  projects: document.getElementById("tab-projects"),
  env: document.getElementById("tab-env"),
  accounts: document.getElementById("tab-accounts"),
  keys: document.getElementById("tab-keys"),
};

const loaded = new Set();

export function showTab(name) {
  for (const button of sidebar.querySelectorAll("button[data-tab]")) {
    if (button.dataset.tab === name) button.setAttribute("aria-current", "page");
    else button.removeAttribute("aria-current");
  }
  for (const [key, panel] of Object.entries(panels)) {
    panel.hidden = key !== name;
  }
  if (!loaded.has(name)) {
    loaded.add(name);
    if (name === "accounts") loadAccounts();
    if (name === "keys") loadKeys();
    if (name === "projects") loadProjects();
  }
}

// 화면 안에서 다른 탭으로 옮겨 달라는 부탁. 탭을 아는 곳은 여기뿐이다.
document.addEventListener("app:show-tab", (event) => showTab(event.detail));

sidebar.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-tab]");
  if (button) showTab(button.dataset.tab);
});

// 작업 기록은 화면이 아니라 아래의 작업 창이다. 지금 보던 화면을 두고 펼친다.
document.getElementById("open-jobs").addEventListener("click", openJobs);
