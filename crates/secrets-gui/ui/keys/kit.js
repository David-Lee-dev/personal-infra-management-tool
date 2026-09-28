// 자격 증명 목록의 공용 조각 — 찾기 · 거르기 줄, 묶음, 한 줄.
//
// 표를 쓰지 않는다. 한 줄은 [이름 · 설명] [상태 칩] [쓰는 곳] 세 덩어리이고, 훑을 때 눈이
// 가는 순서가 곧 이 순서다.

import { span } from "../dom.js";
import { listFilter, listQuery, setListFilter, setListQuery } from "./state.js";

/// 찾기 칸과 거르기 칩. `filters` 는 `{ id, label, count, tone }` — 개수가 0 인 거르기는 흐리게.
export function toolbar({ placeholder, filters = [] }) {
  const bar = span("kl-toolbar", "");
  const search = document.createElement("input");
  search.type = "search";
  search.className = "kl-search";
  search.placeholder = placeholder;
  search.value = listQuery();
  search.spellcheck = false;
  search.autocomplete = "off";
  search.setAttribute("aria-label", placeholder);
  // 다시 그려도 입력이 끊기지 않게, 그린 뒤 커서를 제자리로 돌린다.
  search.addEventListener("input", () => {
    const at = search.selectionStart;
    setListQuery(search.value);
    const again = document.querySelector(".kl-search");
    if (again) {
      again.focus();
      again.setSelectionRange(at, at);
    }
  });
  bar.append(search);

  if (filters.length) {
    const chips = span("kl-filters", "");
    chips.setAttribute("role", "group");
    for (const f of filters) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "kl-filter" + (f.tone ? " " + f.tone : "");
      b.setAttribute("aria-pressed", String(listFilter() === f.id));
      b.append(span("", f.label));
      if (f.count !== undefined) b.append(span("kl-filter-count", String(f.count)));
      if (f.count === 0 && f.id !== "all") b.classList.add("empty");
      b.addEventListener("click", () => setListFilter(listFilter() === f.id ? "all" : f.id));
      chips.append(b);
    }
    bar.append(chips);
  }
  return bar;
}

/// 찾는 말이 이 글들 중 하나에 들어 있는가.
export function matches(...texts) {
  const q = listQuery().trim().toLowerCase();
  if (!q) return true;
  return texts.some((t) => (t ?? "").toLowerCase().includes(q));
}

/// 묶음 — 머리줄(이름 · 개수)과 줄들.
export function group(label, rows, { note } = {}) {
  const box = span("kl-group", "");
  const head = span("kl-group-head", "");
  head.append(span("kl-group-label", label), span("kl-group-count", String(rows.length)));
  if (note) head.append(span("kl-group-note", note));
  box.append(head, ...rows);
  return box;
}

/// 한 줄. `tone` 은 줄 왼쪽 표시 — `warn` 이면 손볼 것이 있다.
export function line({ title, mono = true, sub, chips = [], uses, onClick, tone = "" }) {
  const row = document.createElement("div");
  row.className = "kl-row" + (tone ? " " + tone : "");
  const main = span("kl-main", "");
  main.append(span(mono ? "kl-title mono" : "kl-title", title));
  if (sub) {
    const subline = span("kl-sub", sub);
    subline.title = sub;
    main.append(subline);
  }
  const state = span("kl-chips", "");
  state.append(...chips);
  row.append(main, state, uses ?? span("", ""), span("kl-go", onClick ? "›" : ""));
  if (onClick) {
    row.tabIndex = 0;
    row.setAttribute("role", "button");
    row.addEventListener("click", onClick);
    row.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        onClick();
      }
    });
  }
  return row;
}

export function chip(text, tone = "") {
  return span(tone ? `chip ${tone}` : "chip", text);
}

/// 찾은 것이 없을 때.
export function nothing(text) {
  return span("kl-nothing", text);
}

/// 상세의 한 덩어리 — 제목 줄(오른쪽에 버튼)과 본문.
export function block(title, { actions = [], note } = {}, ...children) {
  const box = document.createElement("section");
  box.className = "kd-block";
  const head = span("kd-block-head", "");
  head.append(span("kd-block-title", title));
  if (note) head.append(span("kd-block-note", note));
  const tools = span("kd-block-tools", "");
  tools.append(...actions);
  head.append(tools);
  box.append(head, ...children.filter(Boolean));
  return box;
}

/// 이름 칸 + 값 칸.
export function slots(rows) {
  const list = document.createElement("dl");
  list.className = "kd-slots";
  for (const [label, ...value] of rows) {
    const dt = document.createElement("dt");
    dt.textContent = label;
    const dd = document.createElement("dd");
    for (const v of value) dd.append(v instanceof Node ? v : document.createTextNode(String(v ?? "—")));
    list.append(dt, dd);
  }
  return list;
}

const EXPIRY_LABEL = {
  expired: (d) => (d === 0 ? "오늘 만료" : `${d}일 전 만료됨`),
  soon: (d) => (d === 0 ? "오늘 만료" : `만료 ${d}일 남음`),
  ok: () => "유효",
  never: () => "기한 없음",
  unset: () => "만료일 모름",
};

/// 만료 판정을 사람이 읽는 말로. 계정 화면과 같은 말을 쓴다.
export function expiryText(expiry) {
  return (EXPIRY_LABEL[expiry.state] ?? (() => expiry.state))(expiry.days ?? 0);
}

/// 알려야 하는 만료만 칩으로 — 곧 만료 · 이미 지남. 나머지는 목록을 어지럽히지 않는다.
export function expiryChips(expiry) {
  return expiry.state === "soon" || expiry.state === "expired" ? [chip(expiryText(expiry), "warn")] : [];
}
