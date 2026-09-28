// 배포 키 만들기. 리포는 git 주소나 로컬 경로에서 읽는다.

import { button, span } from "../../dom.js";
import { termWrite } from "../../terminal.js";
import { accountsOf, select } from "../state.js";

const { invoke } = window.__TAURI__.core;

function field(label, id, { value = "", placeholder = "" } = {}) {
  const wrap = document.createElement("div");
  wrap.className = "field";

  const el = document.createElement("label");
  el.textContent = label;
  el.htmlFor = id;

  const input = document.createElement("input");
  input.id = id;
  input.type = "text";
  input.autocomplete = "off";
  input.spellcheck = false;
  input.value = value;
  input.placeholder = placeholder;

  wrap.append(el, input);
  return { wrap, input };
}

function picker(label, id, options) {
  const wrap = document.createElement("div");
  wrap.className = "field";

  const el = document.createElement("label");
  el.textContent = label;
  el.htmlFor = id;

  const menu = document.createElement("select");
  menu.id = id;
  for (const option of options) {
    const node = document.createElement("option");
    node.value = option;
    node.textContent = option;
    menu.append(node);
  }

  wrap.append(el, menu);
  return { wrap, menu };
}

function choice(label, options) {
  const wrap = document.createElement("div");
  wrap.className = "field";
  wrap.append(span("field-label", label));

  const row = document.createElement("div");
  row.className = "choice-row";
  const inputs = [];
  for (const [index, option] of options.entries()) {
    const item = document.createElement("label");
    item.className = "choice";
    const input = document.createElement("input");
    input.type = "radio";
    input.name = "k-permission";
    input.value = option.value;
    if (index === 0) input.checked = true;
    item.append(input, span("choice-text", option.label));
    row.append(item);
    inputs.push(input);
  }

  wrap.append(row);
  return { wrap, value: () => inputs.find((i) => i.checked)?.value };
}

export function renderNewDeploy(mount) {
  const who = picker("계정", "k-account", accountsOf());

  const source = field("리포지토리", "k-repo", {
    placeholder: "git@github.com:… 또는 리포 디렉토리 경로",
  });

  // 붙여넣은 것이 무엇으로 읽혔는지 그 자리에서 보여 준다.
  const resolved = span("resolved", "");
  const name = field("용도", "k-name", { placeholder: "키를 구분할 용도 이름" });
  const permission = choice("권한", [
    { value: "read", label: "읽기 전용" },
    { value: "write", label: "쓰기 허용" },
  ]);

  // 지금 읽힌 리포. 읽히지 않았으면 null 이고 만들기가 눌리지 않는다.
  let target = null;

  const create = button("만들기", {
    primary: true,
    onClick: async () => {
      if (!target) return;
      create.disabled = true;
      try {
        await invoke("create_deploy_key", {
          account: who.menu.value,
          repo: target.slug,
          purpose: name.input.value.trim(),
          write: permission.value() === "write",
        });
        select(null);
      } catch (err) {
        termWrite("err", String(err));
        create.disabled = false;
      }
    },
  });

  // 해석은 뒷단이 한다 — 경로면 git 에게 origin 을 묻기 때문이다.
  // 입력마다 부르므로 늦게 온 답이 새 입력을 덮지 않게 순번을 단다.
  let turn = 0;
  const reread = async () => {
    const mine = ++turn;
    const text = source.input.value;

    let found = null;
    let problem = "";
    try {
      found = await invoke("resolve_repo", { text });
    } catch (err) {
      problem = String(err);
    }
    if (mine !== turn) return;

    target = found;
    resolved.textContent = found ? `→ ${found.owner} / ${found.name}` : problem;
    resolved.classList.toggle("on", Boolean(found));
    resolved.classList.toggle("bad", Boolean(problem));
    create.disabled = !found;
  };
  source.input.addEventListener("input", reread);
  reread();

  const form = document.createElement("form");
  form.className = "account-form";
  form.autocomplete = "off";
  form.addEventListener("submit", (event) => event.preventDefault());

  const heading = document.createElement("div");
  heading.className = "detail-head";
  const wrap = document.createElement("div");
  wrap.className = "detail-title-wrap";
  const h2 = document.createElement("h2");
  h2.textContent = "배포 키 만들기";
  wrap.append(h2);
  heading.append(wrap);

  const body = document.createElement("div");
  body.className = "detail-body";
  body.append(source.wrap, resolved, who.wrap, name.wrap, permission.wrap);

  const actions = document.createElement("div");
  actions.className = "detail-actions";
  actions.append(button("취소", { onClick: () => select(null) }), create);

  const back = document.createElement("button");
  back.type = "button";
  back.className = "back";
  back.textContent = "‹ 배포 키";
  back.addEventListener("click", () => select(null));

  form.append(heading, body, actions);
  mount.replaceChildren(back, form);
}
