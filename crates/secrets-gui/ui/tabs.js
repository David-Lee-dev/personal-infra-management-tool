// 탭 전환. 탭을 처음 열 때만 데이터를 읽는다.

import { loadAccounts } from "./accounts.js";

const tabBar = document.getElementById("tabs");
const panels = {
  env: document.getElementById("tab-env"),
  accounts: document.getElementById("tab-accounts"),
};

const loaded = new Set();

export function showTab(name) {
  for (const button of tabBar.querySelectorAll("button")) {
    button.setAttribute("aria-selected", String(button.dataset.tab === name));
  }
  for (const [key, panel] of Object.entries(panels)) {
    panel.hidden = key !== name;
  }
  if (!loaded.has(name)) {
    loaded.add(name);
    if (name === "accounts") loadAccounts();
  }
}

tabBar.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-tab]");
  if (button) showTab(button.dataset.tab);
});
