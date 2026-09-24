// 목록 — 폭을 다 쓰는 표.
//
// 열 너비를 직접 정한다. 지문·경로처럼 긴 값이 섞여 있어 자동 계산에 맡기면
// 창 폭에 따라 열이 출렁이고, 긴 값이 옆 칸을 밀어낸다.

import { span } from "../../dom.js";
import { known, permissionText, select, stateText, stateTone, unowned } from "../state.js";

function section(title, count) {
  const head = document.createElement("div");
  head.className = "list-head";
  head.append(span("cap", title), span("list-count", String(count)));
  return head;
}

// 상태는 낱말이자 모양이다. 훑을 때 색으로 먼저 걸린다.
function chip(text, tone) {
  return span(`chip ${tone}`.trim(), text);
}

// 권한은 화살표로. 받는 것과 미는 것이라 git 이 하는 일과 모양이 같고,
// 쓰기가 읽기를 포함한다는 게 낱말보다 분명하다.
function access(key) {
  const box = document.createElement("span");
  box.className = "access";
  box.append(span("access-in", "↓"));
  if (key.write) box.append(span("access-out", "↑"));
  box.title = key.write ? "읽기 · 쓰기 가능" : "읽기만 가능";
  return box;
}

function table(columns, rows) {
  const el = document.createElement("table");
  el.className = "list";

  const group = document.createElement("colgroup");
  for (const column of columns) {
    const col = document.createElement("col");
    col.style.width = column.width;
    group.append(col);
  }
  const last = document.createElement("col");
  last.style.width = "28px";
  group.append(last);
  el.append(group);

  const headRow = document.createElement("tr");
  for (const column of columns) {
    const th = document.createElement("th");
    th.textContent = column.label;
    headRow.append(th);
  }
  headRow.append(document.createElement("th"));

  const thead = document.createElement("thead");
  thead.append(headRow);
  el.append(thead);

  const body = document.createElement("tbody");
  body.append(...rows);
  el.append(body);
  return el;
}

function row(cells, onClick) {
  const tr = document.createElement("tr");
  tr.tabIndex = 0;
  tr.className = "list-row";

  for (const cell of cells) {
    const td = document.createElement("td");
    if (typeof cell === "string") {
      td.textContent = cell;
    } else if (cell.node) {
      td.append(cell.node);
      if (cell.className) td.className = cell.className;
    }
    tr.append(td);
  }

  const chevron = document.createElement("td");
  chevron.className = "list-go";
  chevron.textContent = "›";
  tr.append(chevron);

  tr.addEventListener("click", onClick);
  tr.addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onClick();
    }
  });
  return tr;
}

// 구분되는 것이 앞에 온다. 같은 용도의 키는 이름이 다 같아서, 그것을 첫 열에
// 두면 줄이 전부 똑같아 보인다.
function name(text) {
  return { node: span("list-name", text), className: "strong mono" };
}

function deployRows() {
  return known().map((key) =>
    row(
      [
        name(key.repo),
        { node: span("mono", key.purpose) },
        { node: access(key) },
        { node: chip(stateText(key), stateTone(key) || "ok") },
        { node: span("num", key.created_at) },
      ],
      () => select({ kind: "key", ref: key.ref }),
    ),
  );
}

function orphanRows() {
  return unowned().map((orphan) =>
    row(
      [
        name(orphan.title),
        { node: span("mono", orphan.account) },
        { node: span("mono", orphan.repo ?? "계정 전체") },
        { node: span("mono num", orphan.fingerprint) },
        { node: span("num", orphan.registered_at ?? "—") },
      ],
      () => select({ kind: "unowned", ref: orphan.ref }),
    ),
  );
}

export function renderList(mount) {
  const parts = [section("배포 키", known().length)];

  if (known().length) {
    parts.push(
      table(
        [
          { label: "리포지토리", width: "32%" },
          { label: "용도", width: "24%" },
          { label: "권한", width: "10%" },
          { label: "상태", width: "18%" },
          { label: "만든 날", width: "16%" },
        ],
        deployRows(),
      ),
    );
  } else {
    parts.push(span("list-none", "없음"));
  }

  // 원격을 아직 묻지 않았으면 없다고 말하지 않는다. 모르는 것과 없는 것은 다르다.
  if (unowned().length) {
    parts.push(section("개인 키 없음", unowned().length));
    parts.push(
      table(
        [
          { label: "이름", width: "20%" },
          { label: "계정", width: "17%" },
          { label: "자리", width: "22%" },
          { label: "지문", width: "25%" },
          { label: "등록일", width: "16%" },
        ],
        orphanRows(),
      ),
    );
  }

  mount.replaceChildren(...parts);
}
