// 기타 들이기 — 이미 있는 파일 하나와 그 파일을 여는 값.
//
// 이 도구는 만들지 않고 들이기만 한다. 들이면 원래 파일은 금고로 **옮긴다** — 사본을 남기면
// 흩어진 상태가 그대로다. 값은 금고의 values.env 에만 남고 화면 · 로그에는 이름만 나온다.

import { button, pickOrType, span } from "../../dom.js";
import { termWrite } from "../../terminal.js";
import { choice, field } from "../aws/form.js";
import { select } from "../state.js";
import { KINDS } from "./kinds.js";
import { etcItems } from "./list.js";

const { invoke } = window.__TAURI__.core;

function labeled(label, control) {
  const wrap = document.createElement("div");
  wrap.className = "field";
  wrap.append(span("field-label", label), control);
  return wrap;
}

function valueRow(name, onRemove) {
  const row = document.createElement("div");
  row.className = "pem-row";
  const key = document.createElement("input");
  key.type = "text";
  key.placeholder = "이름";
  key.spellcheck = false;
  key.autocomplete = "off";
  key.value = name;
  key.setAttribute("aria-label", "값 이름");
  const value = document.createElement("input");
  value.type = "password";
  value.placeholder = "값";
  value.autocomplete = "off";
  value.setAttribute("aria-label", "값");
  const remove = button("빼기", { onClick: () => onRemove(row) });
  row.append(key, value, remove);
  return { row, key, value };
}

export function renderAdopt(mount) {
  const groups = [...new Set(etcItems().map((i) => i.project))].sort();
  const group = pickOrType(groups, { placeholder: "그룹 이름" });
  const name = field("이름", "e-name", "자격 증명 이름");
  const kind = choice(
    "종류",
    "e-kind",
    KINDS.map((k) => ({ value: k.id, label: k.label })),
  );
  const purpose = field("용도", "e-purpose", "비워 둘 수 있습니다");

  const path = document.createElement("input");
  path.type = "text";
  path.placeholder = "가져올 파일 경로";
  path.spellcheck = false;
  path.autocomplete = "off";
  path.setAttribute("aria-label", "들일 파일");
  const pick = button("Finder에서 고르기", {
    onClick: async () => {
      try {
        const picked = await invoke("pick_etc_file");
        if (picked) path.value = picked;
      } catch (err) {
        termWrite("err", String(err));
      }
    },
  });
  const fileRow = document.createElement("div");
  fileRow.className = "pem-row";
  fileRow.append(path, pick);

  // 여는 값. 종류를 바꾸면 그 종류의 흔한 이름으로 다시 채운다 — 아직 아무 값도 적지 않았을 때만.
  const rows = [];
  const list = document.createElement("div");
  list.className = "detail-body";
  function addRow(value = "") {
    const made = valueRow(value, (row) => {
      rows.splice(rows.findIndex((r) => r.row === row), 1);
      row.remove();
    });
    rows.push(made);
    list.append(made.row);
  }
  function preset() {
    if (rows.some((r) => r.value.value)) return;
    for (const r of rows.splice(0)) r.row.remove();
    for (const n of KINDS.find((k) => k.id === kind.value())?.values ?? []) addRow(n);
  }
  for (const input of kind.inputs) input.addEventListener("change", preset);
  preset();
  const more = button("＋ 값 추가", { onClick: () => addRow() });

  const problem = span("problem", "");
  problem.hidden = true;
  const create = button("가져오기", { primary: true, onClick: () => adopt() });

  async function adopt() {
    problem.hidden = true;
    create.disabled = true;
    try {
      await invoke("adopt_etc", {
        form: {
          project: group.value().trim(),
          name: name.input.value.trim(),
          kind: kind.value(),
          purpose: purpose.input.value.trim(),
          path: path.value.trim(),
          values: rows
            .filter((r) => r.key.value.trim() || r.value.value)
            .map((r) => ({ name: r.key.value.trim(), value: r.value.value })),
        },
      });
      select(null);
    } catch (err) {
      problem.textContent = String(err);
      problem.hidden = false;
      create.disabled = false;
    }
  }

  const form = document.createElement("form");
  form.className = "account-form";
  form.autocomplete = "off";
  form.addEventListener("submit", (event) => event.preventDefault());

  const heading = document.createElement("div");
  heading.className = "detail-head";
  const wrap = document.createElement("div");
  wrap.className = "detail-title-wrap";
  const h2 = document.createElement("h2");
  h2.textContent = "기타 가져오기";
  wrap.append(h2);
  heading.append(wrap);

  const body = document.createElement("div");
  body.className = "detail-body";
  body.append(
    labeled("그룹", group.node),
    name.wrap,
    kind.wrap,
    purpose.wrap,
    labeled("파일", fileRow),
    span("pane-note", "가져오면 이 파일은 원래 자리에서 시크릿 저장소로 옮겨집니다. 사본을 남기지 않습니다."),
    labeled("여는 값", list),
    more,
    span("pane-note", "값은 시크릿 저장소의 values.env(0600)에만 둡니다. 화면과 작업 로그에는 이름만 나옵니다."),
  );

  const actions = document.createElement("div");
  actions.className = "detail-actions";
  actions.append(problem, button("취소", { onClick: () => select(null) }), create);

  const backLink = document.createElement("button");
  backLink.type = "button";
  backLink.className = "back";
  backLink.textContent = "‹ 기타";
  backLink.addEventListener("click", () => select(null));

  form.append(heading, body, actions);
  mount.replaceChildren(backLink, form);
}
