// 키 화면의 조각들. 도메인이 달라도 모양은 같다.

import { button, span } from "../dom.js";
import { chooser } from "../combo.js";
import { termWrite } from "../terminal.js";
import { select } from "./state.js";

const { invoke } = window.__TAURI__.core;

// 실패는 터미널 칸에 그대로 남긴다. 무엇이 왜 안 됐는지 숨기지 않는다.
export function ask(command, args) {
  return invoke(command, args).catch((err) => {
    termWrite("err", String(err));
    throw err;
  });
}

export function back(label) {
  const el = document.createElement("button");
  el.type = "button";
  el.className = "back";
  el.textContent = `‹ ${label}`;
  el.addEventListener("click", () => select(null));
  return el;
}

export function head(title, sub, { badges = [], buttons = [] }) {
  const box = document.createElement("div");
  box.className = "detail-head";

  const wrap = document.createElement("div");
  wrap.className = "detail-title-wrap";

  const line = document.createElement("div");
  line.className = "detail-title-line";
  const h2 = document.createElement("h2");
  h2.className = "repo-title";
  h2.textContent = title;
  line.append(h2, ...badges);
  wrap.append(line);
  if (sub) wrap.append(span("detail-sub", sub));

  const right = document.createElement("div");
  right.className = "head-actions";
  right.append(...buttons);

  box.append(wrap, right);
  return box;
}

// 용도는 제자리에서 고친다. 따로 화면을 띄우면 무엇을 고치는 중인지 흐려진다.
//
// 저장은 Enter 나 포커스를 뗄 때다. 로컬 디렉토리를 옮기는 일이라 GitHub 은
// 건드리지 않는다.

// 용도는 제자리에서 고친다. 따로 화면을 띄우면 무엇을 고치는 중인지 흐려진다.
//
// 저장은 Enter 나 포커스를 뗄 때다. 로컬 디렉토리를 옮기는 일이라 GitHub 은
// 건드리지 않는다.
export function purposeField(current, commit) {
  const input = document.createElement("input");
  input.className = "inline-edit";
  input.type = "text";
  input.value = current;
  input.spellcheck = false;
  input.setAttribute("aria-label", "이 키의 용도");

  let busy = false;
  const save = async () => {
    const next = input.value.trim();
    if (busy || next === current) return;
    if (!next) {
      input.value = current;
      return;
    }
    busy = true;
    input.disabled = true;
    try {
      await commit(next);
    } catch {
      input.value = current;
    } finally {
      busy = false;
      input.disabled = false;
    }
  };

  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      input.blur();
    }
    if (event.key === "Escape") {
      input.value = current;
      input.blur();
    }
  });
  input.addEventListener("blur", save);
  return input;
}

// 나란히 두면 스크롤 없이 한눈에 든다. 폭이 생겨서 가능해진 배치다.

// 나란히 두면 스크롤 없이 한눈에 든다. 폭이 생겨서 가능해진 배치다.
export function side(...panes) {
  const box = document.createElement("div");
  box.className = "pane-row";
  box.append(...panes);
  return box;
}

// 빈칸을 채우면 명령이 그 자리에서 완성된다. 복사해 바로 붙여넣을 수 있게 —
// `<호스트>` 를 손으로 갈아 끼우게 하면 그 단계에서 틀린다.
export function recipe(fields, build) {
  const box = document.createElement("div");
  box.className = "recipe";

  const row = document.createElement("div");
  row.className = "recipe-fields";

  const inputs = fields.map((spec) => {
    const wrap = document.createElement("div");
    wrap.className = "field";

    const label = document.createElement("label");
    label.textContent = spec.label;
    label.htmlFor = spec.id;

    const input = document.createElement("input");
    input.id = spec.id;
    input.type = "text";
    input.autocomplete = "off";
    input.spellcheck = false;
    input.placeholder = spec.placeholder;

    // 아는 것은 고르고, 모르는 것은 친다. IP 를 바로 넣어도 그대로 쓰인다.
    if (spec.choices) {
      const line = document.createElement("div");
      line.className = "with-chooser";
      line.append(input, chooser(input, spec.choices));
      wrap.append(label, line);
    } else {
      wrap.append(label, input);
    }
    row.append(wrap);
    return { input, fallback: spec.fallback };
  });

  const text = () => build(inputs.map((f) => f.input.value.trim() || f.fallback));
  const shown = command(text());
  const redraw = () => shown.querySelector("pre").replaceChildren(text());
  for (const { input } of inputs) input.addEventListener("input", redraw);

  box.append(row, shown);
  return box;
}

// 무엇이 실행되는지 보이게 두고, 손은 한 번만 가게 한다.

// 무엇이 실행되는지 보이게 두고, 손은 한 번만 가게 한다.
export function command(text) {
  const box = document.createElement("div");
  box.className = "command";

  const pre = document.createElement("pre");
  pre.textContent = text;

  const copy = button("복사", {
    onClick: () => {
      navigator.clipboard.writeText(pre.textContent);
      copy.textContent = "복사됨";
      setTimeout(() => (copy.textContent = "복사"), 1500);
    },
  });
  copy.className = "quiet";

  box.append(pre, copy);
  return box;
}

// 개인 키가 금고 밖으로 나가는 유일한 자리.
//
// 이 맥에서 쓰는 키는 나갈 일이 없다 — 리포의 core.sshCommand 가 금고를 바로
// 가리킨다. 나가는 것은 다른 기계가 쓸 키뿐이다.
