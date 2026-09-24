// 서버 연결 — 확인된 배포 계정을 프로젝트의 환경으로 잇는다.
//
// 인스턴스와 서버 계정은 이 창에서 만들지 않는다. AWS 콘솔에서 인스턴스를 만들고,
// 인프라 › 자격 증명 › AWS 에서 계정을 만들어 확인한 뒤 여기서 고른다.
// 배포 경로는 /srv/<레포 이름> 규칙으로 정해진다. 서버는 읽기만 한다 — 그 경로가 비어 있는지,
// 같은 레포의 checkout 인지.

import { span } from "../dom.js";
import { modal } from "../modal.js";

const { invoke } = window.__TAURI__.core;

function input(id, placeholder) {
  const el = document.createElement("input");
  el.id = id;
  el.type = "text";
  el.placeholder = placeholder;
  el.spellcheck = false;
  el.autocomplete = "off";
  el.className = "mono";
  return el;
}

function field(label, control, help) {
  const box = document.createElement("div");
  box.className = "field";
  const el = document.createElement("label");
  el.textContent = label;
  el.htmlFor = control.id;
  box.append(el, control);
  if (help) box.append(span("field-help", help));
  return box;
}

function choice(group, label, note, disabled, onPick) {
  const row = document.createElement("label");
  row.className = "choice-row";
  const radio = document.createElement("input");
  radio.type = "radio";
  radio.name = group;
  radio.disabled = disabled;
  radio.addEventListener("change", onPick);
  const text = document.createElement("span");
  text.className = "choice-text";
  text.append(span("strong", label));
  if (note) text.append(span("muted small", note));
  row.append(radio, text);
  return { row, radio };
}

function step(number, title, ...children) {
  const box = document.createElement("section");
  box.className = "git-section";
  const h = document.createElement("h3");
  h.textContent = `${number}. ${title}`;
  box.append(h, ...children);
  return box;
}

/// 배포 경로를 읽은 결과 한 줄.
export function checkoutLine(checkout) {
  if (checkout.state === "missing") return "배포 경로가 아직 없습니다. 코드는 아직 받지 않았습니다.";
  if (checkout.state === "empty") return "배포 경로가 빈 디렉토리입니다. 코드는 아직 받지 않았습니다.";
  if (checkout.state === "plain") return "배포 경로에 git 저장소가 아닌 파일이 있습니다.";
  const parts = [checkout.origin ?? "origin 없음", checkout.branch ?? "분리된 HEAD", checkout.commit].filter(Boolean);
  return `같은 레포의 checkout · ${parts.join(" · ")}`;
}

function body(project, plan, close) {
  if (plan.problem) return [span("notice warn", plan.problem)];
  if (!plan.instances.length) {
    return [
      span("notice warn", "연결할 인스턴스가 없습니다."),
      span(
        "pane-note",
        "AWS 콘솔에서 인스턴스를 만든 뒤, 인프라 › 자격 증명 › AWS에서 키 페어를 가져오고 배포 계정을 만들어 접속을 확인하세요. 그다음 여기서 연결합니다.",
      ),
    ];
  }

  let seat = null;

  // 2. 계정 — 인스턴스를 고르면 그 인스턴스의 배포 계정만 보여 준다.
  const accountBox = document.createElement("div");
  accountBox.className = "choice-list";
  accountBox.append(span("muted small", "인스턴스를 먼저 고르세요."));
  function showAccounts(instance) {
    seat = null;
    const usable = instance.accounts.filter((a) => a.verified);
    const rows = instance.accounts.map((account) => {
      const notes = [
        account.verified ? "" : "접속 확인 전 — 자격 증명 화면에서 확인하세요",
        account.used_by.length ? `사용 중: ${account.used_by.join(", ")}` : "",
      ].filter(Boolean);
      const { row, radio } = choice("server-account", account.login, notes.join(" · "), !account.verified, () => {
        seat = account.ref;
      });
      if (usable.length === 1 && account.verified) {
        radio.checked = true;
        seat = account.ref;
      }
      return row;
    });
    const hidden = instance.admins
      ? [span("muted small", `관리 계정 ${instance.admins}개는 배포에 쓰지 않아 목록에서 뺐습니다.`)]
      : [];
    accountBox.replaceChildren(...rows, ...hidden);
  }

  // 1. 인스턴스
  const instanceBox = document.createElement("div");
  instanceBox.className = "choice-list";
  for (const instance of plan.instances) {
    const usable = instance.accounts.filter((a) => a.verified).length;
    const note = usable
      ? `${instance.address} · ${instance.machine} · 배포 계정 ${usable}개`
      : `${instance.address} · ${instance.machine} · 배포 계정 없음 — 자격 증명 › AWS에서 만드세요`;
    const { row } = choice("server-instance", instance.name || instance.instance, note, usable === 0, () =>
      showAccounts(instance),
    );
    instanceBox.append(row);
  }

  // 3. 환경
  const env = input("server-env", "prod, dev …");
  const pathLine = document.createElement("div");
  pathLine.className = "detected-line";
  pathLine.append(span("muted", "배포 경로"), span("mono small", `${plan.deploy_path} — /srv/<레포 이름> 규칙`));

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
  submit.textContent = "확인하고 연결";
  submit.addEventListener("click", async () => {
    problem.hidden = true;
    if (!seat) {
      problem.textContent = "인스턴스와 배포 계정을 고르세요.";
      problem.hidden = false;
      return;
    }
    submit.disabled = true;
    submit.textContent = "서버 확인 중…";
    try {
      const attached = await invoke("attach_server", {
        form: { project: project.name, environment: env.value, seat },
      });
      result.replaceChildren(span("", `${attached.environment.name} 연결됨 — ${checkoutLine(attached.checkout)}`));
      submit.textContent = "닫기";
      submit.disabled = false;
      submit.onclick = close;
    } catch (err) {
      problem.textContent = String(err);
      problem.hidden = false;
      submit.textContent = "확인하고 연결";
      submit.disabled = false;
    }
  });
  actions.append(problem, cancel, submit);

  return [
    span(
      "pane-note",
      "인스턴스와 서버 계정은 여기서 만들지 않습니다. 고른 계정의 키로 서버에 들어가 배포 경로를 읽기만 합니다.",
    ),
    step(1, "인스턴스", instanceBox),
    step(2, "배포 계정", accountBox),
    step(3, "환경", field("환경 이름", env, "로컬의 .env.<환경> 파일과 짝이 됩니다. local과 example은 쓸 수 없습니다."), pathLine),
    result,
    actions,
  ];
}

/// 프로젝트의 서버 연결 창을 연다.
export function openServer(project) {
  modal(`서버 연결 · ${project.name}`, (close) => {
    const holder = document.createElement("div");
    holder.className = "git-form";
    holder.append(span("muted", "서버 계정을 읽는 중…"));
    invoke("server_plan", { project: project.name })
      .then((plan) => holder.replaceChildren(...body(project, plan, close)))
      .catch((err) => holder.replaceChildren(span("problem", String(err))));
    return [holder];
  });
}
