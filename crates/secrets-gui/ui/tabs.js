// 왼쪽 메뉴로 화면을 바꾼다. 화면을 처음 열 때만 데이터를 읽는다.
//
// 화면 이동은 브라우저 기록(history)에 남긴다. 뒤로 · 앞으로(⌘[ · ⌘] · ⌘← · ⌘→ · 마우스 옆 버튼)가
// 브라우저처럼 동작한다. 기록 한 칸은 { tab, project } — 화면과, 프로젝트 상세면 그 이름이다.

import { loadAccounts } from "./accounts/index.js";
import { loadKeys } from "./keys/index.js";
import { loadProjects } from "./projects/index.js";
import { loadSsh } from "./ssh/index.js";
import { openJobs } from "./terminal.js";

const sidebar = document.getElementById("sidebar");
const panels = {
  projects: document.getElementById("tab-projects"),
  ssh: document.getElementById("tab-ssh"),
  env: document.getElementById("tab-env"),
  accounts: document.getElementById("tab-accounts"),
  keys: document.getElementById("tab-keys"),
};

const loaded = new Set();

export function showTab(name) {
  // 앱을 처음 열 때의 화면을 기록의 첫 칸으로 둔다.
  if (!history.state) history.replaceState({ tab: name, project: null }, "");
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
    if (name === "ssh") loadSsh();
  }
}

/// 새 화면으로 간다. 지금 칸과 같으면 기록을 늘리지 않는다.
export function navigate(state) {
  const now = history.state ?? {};
  if (now.tab === state.tab && (now.project ?? null) === (state.project ?? null)) return;
  history.pushState({ tab: state.tab, project: state.project ?? null }, "");
}

// 메뉴를 누르면 그 화면의 처음(목록)으로 간다. 상세를 보고 있었어도 마찬가지다.
sidebar.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-tab]");
  if (!button) return;
  navigate({ tab: button.dataset.tab, project: null });
  showTab(button.dataset.tab);
  document.dispatchEvent(new CustomEvent("app:nav", { detail: button.dataset.tab }));
});

// 뒤로 · 앞으로 — 기록의 그 칸을 다시 그린다. 화면 안의 상태(프로젝트 상세)는 각 화면이 맡는다.
window.addEventListener("popstate", (event) => {
  const state = event.state ?? { tab: "projects", project: null };
  showTab(state.tab);
  document.dispatchEvent(new CustomEvent("app:route", { detail: state }));
});

/// 글을 쓰는 중에는 ⌘← · ⌘→ 가 커서 이동이다. 그때는 가로채지 않는다.
function typing(target) {
  return target instanceof HTMLElement && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName));
}

document.addEventListener("keydown", (event) => {
  if (!event.metaKey || event.altKey || event.ctrlKey) return;
  const back = event.key === "[" || (event.key === "ArrowLeft" && !typing(event.target));
  const forward = event.key === "]" || (event.key === "ArrowRight" && !typing(event.target));
  if (!back && !forward) return;
  event.preventDefault();
  if (back) history.back();
  else history.forward();
});

// 마우스 옆 버튼(뒤로 3 · 앞으로 4).
window.addEventListener("mouseup", (event) => {
  if (event.button === 3) {
    event.preventDefault();
    history.back();
  } else if (event.button === 4) {
    event.preventDefault();
    history.forward();
  }
});

// 작업 기록은 화면이 아니라 아래의 작업 창이다. 지금 보던 화면을 두고 펼친다.
document.getElementById("open-jobs").addEventListener("click", openJobs);
