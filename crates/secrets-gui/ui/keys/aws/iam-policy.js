// IAM 정책 바꾸기.
//
// 지금 정책을 채워 두고 고치게 한다. 치는 대로 뒷단이 읽어 새 규칙과 규칙 위반, 지금
// 정책과 무엇이 다른지를 돌려준다. 서비스가 바뀌면 이름의 권한 조각과 어긋나므로 새 IAM
// 발급을 권한다(규칙 9) — 막지는 않는다.
//
// 바꾸면 AWS 의 인라인 정책을 바꾸고 시뮬레이터로 다시 확인한다. 확인이 실패하면 뒷단이
// 이전 정책으로 되돌린다. 키는 그대로다. 이전 정책은 금고의 history/ 에 남는다.

import { button, span } from "../../dom.js";
import { ask } from "../parts.js";
import { select } from "../state.js";
import { row, table } from "../table.js";

const { invoke } = window.__TAURI__.core;

function mono(text) {
  const el = span("mono", text);
  el.title = text;
  return el;
}

function whereOf(user) {
  return { account: user.account, name: user.name };
}

export function renderIamPolicy(mount, user) {
  const policyWrap = document.createElement("div");
  policyWrap.className = "field";
  const policyLabel = document.createElement("label");
  policyLabel.textContent = "정책";
  policyLabel.htmlFor = "ip-policy";
  const policy = document.createElement("textarea");
  policy.id = "ip-policy";
  policy.rows = 16;
  policy.spellcheck = false;
  policyWrap.append(policyLabel, policy);

  const reading = document.createElement("div");
  reading.className = "policy-reading";

  // 첫 누름은 무엇이 바뀌는지 확인시키고, 두 번째 누름이 AWS 에 쓴다.
  let armed = false;
  const change = button("정책 바꾸기", { primary: true, onClick: () => submit() });
  change.disabled = true;
  const disarm = () => {
    armed = false;
    change.textContent = "정책 바꾸기";
  };

  let asked = 0;
  let timer = null;
  function schedule() {
    disarm();
    change.disabled = true;
    clearTimeout(timer);
    timer = setTimeout(refresh, 150);
  }

  async function refresh() {
    const mine = ++asked;
    const seen = await invoke("plan_iam_policy", { at: whereOf(user), policy: policy.value }).catch((err) => ({
      rules: [],
      problems: [],
      error: String(err),
    }));
    if (mine !== asked) return;

    const parts = [];
    if (seen.error) parts.push(span("muted", seen.error));
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
    if (seen.service_changed) {
      parts.push(
        span(
          "notice warn",
          `서비스가 바뀝니다. 이름의 권한 조각(${user.perm || "—"})과 어긋나므로 새 IAM을 발급해 옮기는 것을 권합니다(규칙 9).`,
        ),
      );
    } else if (seen.resources_changed) {
      parts.push(span("notice", "허용 대상이 바뀝니다. 이 키를 쓰는 곳이 새 대상으로 충분한지 확인하세요."));
    }
    reading.replaceChildren(...parts);
    change.disabled = Boolean(seen.error);
  }

  async function submit() {
    if (!armed) {
      armed = true;
      change.textContent = "AWS에 적용";
      return;
    }
    change.disabled = true;
    policy.disabled = true;
    try {
      const changed = await ask("change_iam_policy", { at: whereOf(user), policy: policy.value });
      select({ kind: "iam", ref: changed.ref });
    } catch {
      disarm();
      change.disabled = false;
      policy.disabled = false;
    }
  }

  policy.addEventListener("input", schedule);

  const form = document.createElement("form");
  form.className = "account-form";
  form.autocomplete = "off";
  form.addEventListener("submit", (event) => event.preventDefault());

  const heading = document.createElement("div");
  heading.className = "detail-head";
  const wrap = document.createElement("div");
  wrap.className = "detail-title-wrap";
  const h2 = document.createElement("h2");
  h2.textContent = "정책 바꾸기";
  wrap.append(h2);
  heading.append(wrap, span("detail-sub", `${user.name} · ${user.account}`));

  const body = document.createElement("div");
  body.className = "detail-body";
  body.append(
    policyWrap,
    reading,
    span(
      "pane-note",
      "AWS의 인라인 정책을 바꾸고 시뮬레이터로 다시 확인합니다. 확인이 실패하면 이전 정책으로 되돌립니다. 키는 그대로이고, 이전 정책은 시크릿 저장소의 history/에 남습니다.",
    ),
  );

  const actions = document.createElement("div");
  actions.className = "detail-actions";
  const toDetail = () => select({ kind: "iam", ref: user.ref });
  actions.append(button("취소", { onClick: toDetail }), change);

  const backLink = document.createElement("button");
  backLink.type = "button";
  backLink.className = "back";
  backLink.textContent = `‹ ${user.name}`;
  backLink.addEventListener("click", toDetail);

  form.append(heading, body, actions);
  mount.replaceChildren(backLink, form);

  policy.value = "정책을 읽는 중…";
  policy.disabled = true;
  invoke("iam_policy_text", { at: whereOf(user) })
    .then((text) => {
      policy.value = text;
      policy.disabled = false;
      refresh();
    })
    .catch((err) => reading.replaceChildren(span("problem", String(err))));
}
