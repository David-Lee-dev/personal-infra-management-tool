// 서버 목록 — 그룹별로 묶은 줄. 계정 칩을 누르면 접속하고, 이름을 누르면 상세로 간다.

import { button, span } from "../dom.js";
import { accountChip, copyCommand } from "./connect.js";
import { KIND_LABEL } from "./form.js";
import { openRegister } from "./register.js";
import { openSuggestions } from "./suggest.js";

let query = "";
let filter = "all";

function head(listed, handlers) {
  const bar = span("sv-head", "");
  const title = document.createElement("h1");
  title.textContent = "서버";
  const accounts = listed.servers.reduce((n, s) => n + s.accounts.length, 0);
  bar.append(
    title,
    span("sv-count", `${listed.servers.length}대 · 계정 ${accounts}개`),
    span("inner-tabs-gap", ""),
    button("＋ 서버 등록", {
      primary: true,
      onClick: () => openRegister({ groups: listed.groups, onDone: handlers.onRegistered }),
    }),
  );
  return bar;
}

function banner(count) {
  const box = span("sv-banner", "");
  box.append(
    span("strong", `등록하지 않은 서버 ${count}대`),
    span("sv-banner-note", "서버 계정 기록과 ~/.ssh/config에서 찾았습니다. 등록하기 전에는 아무것도 바꾸지 않습니다."),
    button("살펴보기", { onClick: openSuggestions }),
  );
  return box;
}

function toolbar(listed, redraw) {
  const bar = span("kl-toolbar", "");
  const search = document.createElement("input");
  search.type = "search";
  search.className = "kl-search";
  search.placeholder = "이름 · 주소 · 계정 · 프로젝트로 찾기";
  search.setAttribute("aria-label", search.placeholder);
  search.value = query;
  search.spellcheck = false;
  search.autocomplete = "off";
  search.addEventListener("input", () => {
    query = search.value;
    redraw();
  });

  const counts = {
    all: listed.servers.length,
    aws: listed.servers.filter((s) => s.kind !== "other").length,
    other: listed.servers.filter((s) => s.kind === "other").length,
  };
  const chips = span("kl-filters", "");
  chips.setAttribute("role", "group");
  for (const [id, label] of [["all", "전체"], ["aws", "AWS"], ["other", "기타"]]) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "kl-filter" + (counts[id] === 0 && id !== "all" ? " empty" : "");
    b.setAttribute("aria-pressed", String(filter === id));
    b.append(span("", label), span("kl-filter-count", String(counts[id])));
    b.addEventListener("click", () => {
      filter = filter === id ? "all" : id;
      redraw();
    });
    chips.append(b);
  }
  bar.append(search, chips);
  return { bar, search };
}

function shown(server) {
  if (filter === "aws" && server.kind === "other") return false;
  if (filter === "other" && server.kind !== "other") return false;
  const q = query.trim().toLowerCase();
  if (!q) return true;
  const texts = [
    server.name,
    server.address,
    ...server.accounts.map((a) => a.login),
    ...server.uses.map((u) => `${u.project} ${u.environment}`),
  ];
  return texts.some((t) => t.toLowerCase().includes(q));
}

/// 쓰는 곳 — 프로젝트마다 한 칩, 환경을 꼬리로.
function useChips(uses) {
  const box = span("use-chips", "");
  const byProject = new Map();
  for (const u of uses) {
    if (!byProject.has(u.project)) byProject.set(u.project, []);
    byProject.get(u.project).push(u.environment);
  }
  for (const [project, envs] of byProject) {
    const chip = span("use-chip", "");
    chip.append(span("use-project", project), ...envs.map((e) => span("use-env", e)));
    box.append(chip);
  }
  if (!byProject.size) box.append(span("use-none", "쓰는 곳 없음"));
  return box;
}

/// 계정 칩 옆 ⋯ — 계정마다 ssh 명령 복사.
function copyMenu(server) {
  const more = document.createElement("details");
  more.className = "sv-more";
  const summary = document.createElement("summary");
  summary.textContent = "⋯";
  summary.setAttribute("aria-label", `${server.name} ssh 명령 복사`);
  const menu = span("sv-more-menu", "");
  for (const account of server.accounts) {
    const item = button(`ssh 명령 복사 · ${account.login}`, {
      onClick: async () => {
        const copied = await copyCommand(server.id, account.login).catch(() => null);
        if (copied !== null) item.textContent = copied ? `복사됨 · ${account.login}` : "작업 로그에 적었습니다";
      },
    });
    menu.append(item);
  }
  more.append(summary, menu);
  return more;
}

function row(server, onOpen) {
  const line = document.createElement("div");
  line.className = "sv-row";

  const main = document.createElement("button");
  main.type = "button";
  main.className = "sv-main";
  main.addEventListener("click", () => onOpen(server.id));
  const address = server.port === 22 ? server.address : `${server.address}:${server.port}`;
  main.append(span("kl-title mono", server.name), span("kl-sub mono", address));

  const kind = span(`chip sv-kind ${server.kind === "other" ? "other" : "aws"}`, KIND_LABEL[server.kind] ?? server.kind);
  const accounts = span("sv-accounts", "");
  for (const account of server.accounts) accounts.append(accountChip(server.id, account));
  if (!server.accounts.length) accounts.append(span("use-none", "계정 없음"));
  else accounts.append(copyMenu(server));

  line.append(main, kind, accounts, useChips(server.uses));
  return line;
}

export function renderList(mount, listed, handlers) {
  const draw = () => {
    const { bar, search } = toolbar(listed, () => {
      const at = search.selectionStart;
      draw();
      const again = mount.querySelector(".kl-search");
      again?.focus();
      again?.setSelectionRange(at, at);
    });
    const parts = [head(listed, handlers)];
    if (listed.suggestions) parts.push(banner(listed.suggestions));
    parts.push(bar);

    const visible = listed.servers.filter(shown);
    if (!listed.servers.length) {
      parts.push(span("kl-nothing", "등록한 서버가 없습니다. ＋ 서버 등록으로 접속할 서버를 기록하세요."));
    } else if (!visible.length) {
      parts.push(span("kl-nothing", "찾는 서버가 없습니다."));
    }
    const groups = [...new Set(visible.map((s) => s.group))];
    for (const group of groups) {
      const rows = visible.filter((s) => s.group === group).map((s) => row(s, handlers.onOpen));
      const box = span("kl-group", "");
      const title = span("kl-group-head", "");
      title.append(span("kl-group-label", group || "그룹 없음"), span("kl-group-count", String(rows.length)));
      box.append(title, ...rows);
      parts.push(box);
    }
    parts.push(span("kl-foot", "계정을 누르면 Ghostty 새 창에서 그 계정으로 접속합니다. 서버 이름을 누르면 계정 관리 · 쓰는 곳을 봅니다."));
    mount.replaceChildren(...parts);
  };
  draw();
}
