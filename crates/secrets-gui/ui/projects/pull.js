// 코드 받기 — 연결된 환경의 배포 경로로 그 환경의 브랜치를 clone 한다.
//
// 서버에 쓰는 일이라 먼저 무엇을 어디에 둘지 보여 주고, 확인을 받은 뒤 실행한다.
// 처음 한 번만 한다. 창을 열 때 배포 경로를 읽어, 비어 있거나 없을 때만 받기를 보여 준다.
// 이미 받은 코드를 최신으로 맞추는 일은 [배포]가 한다.

import { span } from "../dom.js";
import { modal } from "../modal.js";
import { checkoutLine } from "./server.js";

const { invoke } = window.__TAURI__.core;

/// 이미 받았거나 받을 수 없는 배포 경로면 그 이유. 비어 있거나 없으면 null.
function blocked(checkout) {
  if (checkout.state === "repository") {
    return `이미 받았습니다 — ${checkoutLine(checkout)}. 최신 코드는 [배포]가 받습니다.`;
  }
  if (checkout.state === "plain") {
    return `${checkoutLine(checkout)} 비어 있는 배포 경로에만 받습니다. 서버의 파일은 건드리지 않습니다.`;
  }
  return null;
}

function body(project, plan, checkout, close) {
  if (plan.problem) return [span("notice warn", plan.problem)];
  const reason = blocked(checkout);
  if (reason) {
    const done = document.createElement("button");
    done.type = "button";
    done.textContent = "닫기";
    done.addEventListener("click", close);
    const actions = document.createElement("div");
    actions.className = "modal-actions";
    actions.append(done);
    return [span("notice", reason), actions];
  }
  const env = plan.environment;
  const repoName = plan.repo.split("/")[1];
  // 서버에 둘 키는 자격 증명 › GitHub 에서 발급한 저장된 키 중에서 고른다. 여기서는 만들지 않는다.
  let key = plan.keys.find((k) => k.usable)?.purpose ?? null;

  const keyBox = document.createElement("div");
  keyBox.className = "choice-list";
  for (const k of plan.keys) {
    const row = document.createElement("label");
    row.className = "choice-row";
    const radio = document.createElement("input");
    radio.type = "radio";
    radio.name = "pull-key";
    radio.checked = k.purpose === key;
    radio.disabled = !k.usable;
    radio.addEventListener("change", () => {
      key = k.purpose;
    });
    const text = document.createElement("span");
    text.className = "choice-text";
    text.append(
      span("strong", `저장된 키 · ${k.purpose}`),
      span("muted small", [k.write ? "쓰기 권한" : "읽기 전용", `계정 ${k.account}`, k.usable ? "" : "GitHub 등록이 끝나지 않음"].filter(Boolean).join(" · ")),
    );
    row.append(radio, text);
    keyBox.append(row);
  }
  if (!plan.keys.some((k) => k.usable)) {
    keyBox.append(
      span("notice warn", `${plan.repo}의 쓸 수 있는 저장된 키가 없습니다. 자격 증명 › GitHub에서 읽기 전용 배포 키를 발급한 뒤 다시 여세요.`),
    );
  }

  const summary = document.createElement("dl");
  summary.className = "git-summary";
  for (const [label, value] of [
    ["서버", `${env.server_name} · ${env.login}@${env.address}`],
    ["받을 곳", `${env.path} — 비어 있을 때만 받습니다`],
    ["레포", `${plan.repo} · ${env.branch} 브랜치`],
    ["서버에 두는 것", `${env.login}의 ~/.ssh/github/${repoName} (0600) · 이 레포의 core.sshCommand`],
  ]) {
    const dt = document.createElement("dt");
    dt.textContent = label;
    const dd = document.createElement("dd");
    dd.textContent = value;
    summary.append(dt, dd);
  }

  const result = document.createElement("div");
  result.className = "detected";
  const actions = document.createElement("div");
  actions.className = "modal-actions";
  const problem = span("problem", "");
  problem.hidden = true;
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.textContent = "취소";
  cancel.addEventListener("click", close);
  const submit = document.createElement("button");
  submit.type = "button";
  submit.className = "primary";
  submit.textContent = "코드 받기";
  submit.disabled = key === null;
  submit.addEventListener("click", async () => {
    problem.hidden = true;
    submit.disabled = true;
    submit.textContent = "받는 중…";
    try {
      const pulled = await invoke("pull_code", {
        form: { project: project.name, environment: env.name, key },
      });
      const head = pulled.already ? "이미 받아 둔 상태라 아무것도 하지 않았습니다." : "받았습니다.";
      result.replaceChildren(span("", `${head} ${checkoutLine(pulled.checkout)}`));
      submit.textContent = "닫기";
      submit.disabled = false;
      submit.onclick = close;
    } catch (err) {
      problem.textContent = String(err);
      problem.hidden = false;
      submit.textContent = "코드 받기";
      submit.disabled = false;
    }
  });
  actions.append(problem, cancel, submit);

  return [
    summary,
    field("서버에 둘 키", keyBox),
    span("pane-note", "키 값은 명령 인자나 작업 로그에 남지 않습니다. 배포 경로에 다른 파일이 있으면 아무것도 바꾸지 않고 멈춥니다."),
    result,
    actions,
  ];
}

function field(label, control) {
  const box = document.createElement("div");
  box.className = "field";
  const el = document.createElement("label");
  el.textContent = label;
  box.append(el, control);
  return box;
}

/// 환경 하나의 코드 받기 창을 연다.
export function openPull(project, environment) {
  modal(`코드 받기 · ${project.name} · ${environment}`, (close) => {
    const holder = document.createElement("div");
    holder.className = "git-form";
    holder.append(span("muted", "배포 경로를 읽는 중…"));
    Promise.all([
      invoke("pull_plan", { project: project.name, environment }),
      invoke("check_environment", { project: project.name, environment }),
    ])
      .then(([plan, checkout]) => holder.replaceChildren(...body(project, plan, checkout, close)))
      .catch((err) => holder.replaceChildren(span("problem", String(err))));
    return [holder];
  });
}
