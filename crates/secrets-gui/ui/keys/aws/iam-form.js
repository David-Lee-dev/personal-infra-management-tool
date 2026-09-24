// IAM 만들기.
//
// 기존 사용자는 고르지 않는다. 새로 만들기만 한다. 권한은 정책 JSON 그대로 받는다 —
// 꼴을 미리 정해 두면 그 밖의 권한을 줄 수 없다. 대신 치는 대로 읽어 무엇을
// 허용하는지 보여 주고, 규칙에서 벗어나면 그 자리에서 짚는다.
//
// 읽기와 이름 짓기는 뒷단이 한다. 이름 규칙이 두 벌이 되지 않게 하기 위해서다.

import { button, span } from "../../dom.js";
import { chooser } from "../../combo.js";
import { ask } from "../parts.js";
import { select } from "../state.js";
import { choice, field } from "./form.js";
import { iamUsers } from "./state.js";
import { row, table } from "../table.js";

const { invoke } = window.__TAURI__.core;

const EXAMPLE = `{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": ["s3:PutObject", "s3:GetObject"],
      "Resource": "arn:aws:s3:::tuk-public-320042238085-ap-northeast-2-an/*"
    }
  ]
}`;

function mono(text) {
  const el = span("mono", text);
  el.title = text;
  return el;
}

export function renderIamRegister(mount, master) {
  if (!master?.account) {
    mount.replaceChildren(
      span("problem", "AWS 마스터 계정이 없습니다. 계정 관리 탭에서 먼저 등록하세요."),
    );
    return;
  }

  const app = field("앱", "i-app", "tuk-api");
  const appLine = document.createElement("div");
  appLine.className = "with-chooser";
  appLine.append(
    app.input,
    chooser(app.input, {
      title: "앱 고르기",
      load: async () =>
        [...new Set(iamUsers().map((user) => user.app))].sort().map((value) => ({ value, detail: "" })),
    }),
  );
  app.wrap.replaceChildren(app.wrap.querySelector("label"), appLine);

  const env = choice("환경", "i-env", [
    { value: "prod", label: "prod" },
    { value: "dev", label: "dev" },
    { value: "local", label: "local" },
  ]);
  const purpose = field("용도", "i-purpose", "이미지 업로드");

  // 이름에 들어갈 권한 조각. 비우면 정책에서 정한다.
  const perm = field("권한 이름", "i-perm", "비우면 정책에서 정합니다");

  const policyWrap = document.createElement("div");
  policyWrap.className = "field";
  const policyLabel = document.createElement("label");
  policyLabel.textContent = "정책";
  policyLabel.htmlFor = "i-policy";
  const policy = document.createElement("textarea");
  policy.id = "i-policy";
  policy.rows = 14;
  policy.spellcheck = false;
  policy.placeholder = EXAMPLE;
  policyWrap.append(policyLabel, policy);

  const reading = document.createElement("div");
  reading.className = "policy-reading";

  const name = span("iam-name empty", "앱과 정책을 채우면 정해집니다");
  const nameField = document.createElement("div");
  nameField.className = "field";
  nameField.append(span("field-label", "이름"), name);

  const create = button("만들고 키 발급", { primary: true, onClick: () => submit() });
  create.disabled = true;

  function draft() {
    return {
      master: master.slug,
      account: master.account,
      app: app.input.value.trim(),
      env: env.value(),
      perm: perm.input.value.trim(),
      purpose: purpose.input.value.trim(),
      policy: policy.value,
    };
  }

  // 치는 사이사이 묻는다. 늦게 온 옛 답이 새 답을 덮지 않게 순번을 본다.
  let asked = 0;
  let timer = null;
  function schedule() {
    clearTimeout(timer);
    timer = setTimeout(refresh, 150);
  }

  async function refresh() {
    const mine = ++asked;
    const seen = await invoke("preview_iam", { draft: draft() }).catch((err) => ({
      name: null,
      rules: [],
      problems: [],
      error: String(err),
    }));
    if (mine !== asked) return;

    const parts = [];
    if (seen.rules.length) {
      parts.push(
        table(
          [
            { label: "", width: "8%" },
            { label: "동작", width: "34%" },
            { label: "대상", width: "44%" },
            { label: "조건", width: "14%" },
          ],
          seen.rules.map((r) => row([r.effect, { node: mono(r.actions) }, { node: mono(r.target) }, r.condition])),
        ),
      );
    }
    for (const problem of seen.problems) parts.push(span("problem", problem));
    reading.replaceChildren(...parts);

    name.textContent = seen.name ?? (seen.error || "앱과 정책을 채우면 정해집니다");
    name.classList.toggle("empty", !seen.name);
    create.disabled = !seen.name;
  }

  for (const input of [app.input, perm.input, policy]) input.addEventListener("input", schedule);
  for (const input of env.inputs) input.addEventListener("change", schedule);

  async function submit() {
    create.disabled = true;
    try {
      const made = await ask("create_iam", { draft: draft() });
      select({ kind: "iam", ref: made.ref });
    } catch {
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
  h2.textContent = "IAM 만들기";
  wrap.append(h2);
  heading.append(wrap, span("detail-sub", `${master.account} · ${master.slug}`));

  const body = document.createElement("div");
  body.className = "detail-body";
  body.append(app.wrap, env.wrap, purpose.wrap, policyWrap, reading, perm.wrap, nameField);

  const actions = document.createElement("div");
  actions.className = "detail-actions";
  actions.append(button("취소", { onClick: () => select(null) }), create);

  const backLink = document.createElement("button");
  backLink.type = "button";
  backLink.className = "back";
  backLink.textContent = "‹ AWS";
  backLink.addEventListener("click", () => select(null));

  form.append(heading, body, actions);
  mount.replaceChildren(backLink, form);
}
