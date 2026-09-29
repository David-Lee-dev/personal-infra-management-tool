// 배포 스크립트 — 환경마다 하나. 배포는 반드시 이 스크립트로 한다.
//
// 서버마다 런타임과 도구가 달라 이 도구는 배포 방법을 정하지 않는다. 스크립트는 사용자가
// 빈 칸에서 직접 쓴다.

import { span } from "../dom.js";
import { modal } from "../modal.js";
import { invoke } from "../ipc.js";

function field(label, control) {
  const box = document.createElement("div");
  box.className = "field";
  const el = document.createElement("label");
  el.textContent = label;
  box.append(el, control);
  return box;
}

function factList(rows) {
  const list = document.createElement("dl");
  list.className = "git-summary";
  for (const [label, value] of rows) {
    const dt = document.createElement("dt");
    dt.textContent = label;
    const dd = document.createElement("dd");
    dd.textContent = value;
    list.append(dt, dd);
  }
  return list;
}

function body(project, env, found, close) {
  let saved = found.text ?? "";

  const editor = document.createElement("textarea");
  editor.className = "script-editor";
  editor.rows = 18;
  editor.spellcheck = false;
  editor.value = saved;
  editor.placeholder = "#!/usr/bin/env bash\n# 이 환경을 배포하는 명령을 적습니다.";
  // Tab 은 칸을 벗어나지 않고 공백 두 칸을 넣는다.
  editor.addEventListener("keydown", (event) => {
    if (event.key !== "Tab" || event.shiftKey) return;
    event.preventDefault();
    editor.setRangeText("  ", editor.selectionStart, editor.selectionEnd, "end");
    editor.dispatchEvent(new Event("input"));
  });

  const scriptField = field("스크립트", editor);
  scriptField.classList.add("script-field");

  const variables = document.createElement("dl");
  variables.className = "fact-list script-variables";
  for (const [name, meaning] of found.variables) {
    const dt = document.createElement("dt");
    dt.className = "mono";
    dt.textContent = "$" + name;
    const dd = document.createElement("dd");
    dd.textContent = meaning;
    variables.append(dt, dd);
  }

  const status = span("muted small", found.text == null ? "아직 스크립트가 없습니다." : "");
  const problem = span("problem", "");
  problem.hidden = true;
  const save = document.createElement("button");
  save.type = "button";
  save.className = "primary";
  save.textContent = "저장";
  save.disabled = true;
  const shut = document.createElement("button");
  shut.type = "button";
  shut.textContent = "닫기";

  const dirty = () => editor.value !== saved;
  editor.addEventListener("input", () => {
    save.disabled = !dirty();
    shut.textContent = "닫기";
    if (dirty()) status.textContent = "저장하지 않은 변경이 있습니다.";
  });

  save.addEventListener("click", async () => {
    problem.hidden = true;
    save.disabled = true;
    try {
      const result = await invoke("save_deploy_script", {
        project: project.name,
        environment: env.name,
        text: editor.value,
      });
      saved = editor.value;
      status.textContent = result.unchanged
        ? "바뀐 내용이 없습니다."
        : result.archived
          ? "저장했습니다. 이전 스크립트는 " + result.archived + "에 있습니다."
          : "저장했습니다.";
    } catch (err) {
      problem.textContent = String(err);
      problem.hidden = false;
      save.disabled = !dirty();
    }
  });

  // 저장하지 않은 변경이 있으면 한 번 더 눌러야 닫힌다.
  shut.addEventListener("click", () => {
    if (dirty() && shut.textContent === "닫기") {
      status.textContent = "저장하지 않은 변경이 있습니다. 한 번 더 누르면 버리고 닫습니다.";
      shut.textContent = "버리고 닫기";
      return;
    }
    close();
  });

  const actions = document.createElement("div");
  actions.className = "modal-actions";
  actions.append(problem, shut, save);

  return [
    factList([
      ["파일", found.path],
      ["실행", `서버에서 ${env.login} 계정으로 bash — ${env.login}@${env.address}`],
      ["배포 경로", env.path],
    ]),
    scriptField,
    span("detail-subhead", "스크립트가 받는 환경 변수"),
    variables,
    span("pane-note", "배포는 이 스크립트로만 합니다. 실패하면(0이 아닌 종료 코드) 배포 실패로 봅니다. `## 제목`으로 시작하는 출력 줄은 배포 로그에서 단계 제목으로 보입니다. 파일을 편집기로 직접 고쳐도 됩니다."),
    status,
    actions,
  ];
}

/// 환경 하나의 배포 스크립트 창을 연다.
export function openDeployScript(project, environment) {
  const env = project.environments.find((e) => e.name === environment);
  modal(
    `배포 스크립트 · ${project.name} · ${environment}`,
    (close) => {
      const holder = document.createElement("div");
      holder.className = "modal-fill script-form";
      holder.append(span("muted", "읽는 중…"));
      invoke("deploy_script", { project: project.name, environment })
        .then((found) => holder.replaceChildren(...body(project, env, found, close)))
        .catch((err) => holder.replaceChildren(span("problem", String(err))));
      return [holder];
    },
    { size: "xl", fill: true },
  );
}
