// 배포 스크립트 — 환경마다 이름 붙인 스크립트 여러 개. 배포는 반드시 그중 하나로 한다.
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

/// 스크립트 고르기. 맨 끝의 "새 스크립트"를 고르면 이름 칸이 열린다.
function scriptPicker() {
  const select = document.createElement("select");
  return {
    select,
    /// 이름들로 다시 채운다. `current` 가 null 이면 새 스크립트.
    refill(names, current) {
      select.replaceChildren(
        ...names.map((name) => new Option(name, name)),
        new Option("＋ 새 스크립트", ""),
      );
      select.value = current ?? "";
    },
    chosen: () => select.value || null,
  };
}

function body(project, env, listing, close) {
  let names = listing.names;
  // 고른 스크립트 이름. 새 스크립트면 null.
  let current = null;
  let saved = "";

  const picker = scriptPicker();
  const nameInput = document.createElement("input");
  nameInput.type = "text";
  nameInput.placeholder = "예: deploy · migrate · restart";
  nameInput.spellcheck = false;
  nameInput.autocomplete = "off";
  const nameField = field("새 스크립트 이름", nameInput);
  const pickerField = field("스크립트", picker.select);
  const heading = document.createElement("div");
  heading.className = "field-row";
  heading.append(pickerField, nameField);

  const where = document.createElement("div");

  const editor = document.createElement("textarea");
  editor.className = "script-editor";
  editor.rows = 18;
  editor.spellcheck = false;
  editor.placeholder = "#!/usr/bin/env bash\n# 이 환경에서 돌릴 명령을 적습니다.";
  // Tab 은 칸을 벗어나지 않고 공백 두 칸을 넣는다.
  editor.addEventListener("keydown", (event) => {
    if (event.key !== "Tab" || event.shiftKey) return;
    event.preventDefault();
    editor.setRangeText("  ", editor.selectionStart, editor.selectionEnd, "end");
    editor.dispatchEvent(new Event("input"));
  });

  const scriptField = field("내용", editor);
  scriptField.classList.add("script-field");

  const variables = document.createElement("dl");
  variables.className = "fact-list script-variables";
  for (const [name, meaning] of listing.variables) {
    const dt = document.createElement("dt");
    dt.className = "mono";
    dt.textContent = "$" + name;
    const dd = document.createElement("dd");
    dd.textContent = meaning;
    variables.append(dt, dd);
  }

  const status = span("muted small", "");
  const problem = span("problem", "");
  problem.hidden = true;
  const remove = document.createElement("button");
  remove.type = "button";
  remove.textContent = "빼기";
  const save = document.createElement("button");
  save.type = "button";
  save.className = "primary";
  save.textContent = "저장";
  save.disabled = true;
  const shut = document.createElement("button");
  shut.type = "button";
  shut.textContent = "닫기";

  const dirty = () => editor.value !== saved;
  // 저장하지 않은 변경이 있으면 다른 스크립트로 넘어가거나 닫을 때 한 번 더 확인한다.
  let leaving = false;
  const settle = () => {
    save.disabled = !dirty();
    shut.textContent = "닫기";
    leaving = false;
    remove.textContent = "빼기";
  };

  function whereRows(path) {
    where.replaceChildren(
      factList([
        ["파일", path],
        ["실행", `서버에서 ${env.login} 계정으로 bash — ${env.login}@${env.address}`],
        ["배포 경로", env.path],
      ]),
    );
  }

  async function show(name) {
    current = name;
    picker.refill(names, current);
    nameField.hidden = name != null;
    remove.hidden = name == null;
    problem.hidden = true;
    if (name == null) {
      saved = "";
      editor.value = "";
      nameInput.value = "";
      whereRows(`이름을 정하면 …/deploy/${env.name}/<이름>.sh`);
      status.textContent = names.length ? "새 스크립트를 씁니다." : "아직 스크립트가 없습니다.";
      settle();
      return;
    }
    editor.disabled = true;
    status.textContent = "";
    try {
      const found = await invoke("deploy_script", { project: project.name, environment: env.name, script: name });
      saved = found.text ?? "";
      editor.value = saved;
      whereRows(found.path);
    } catch (err) {
      problem.textContent = String(err);
      problem.hidden = false;
    } finally {
      editor.disabled = false;
      settle();
    }
  }

  editor.addEventListener("input", () => {
    settle();
    if (dirty()) status.textContent = "저장하지 않은 변경이 있습니다.";
  });

  picker.select.addEventListener("change", () => {
    const next = picker.chosen();
    if (dirty() && !leaving) {
      picker.select.value = current ?? "";
      status.textContent = "저장하지 않은 변경이 있습니다. 한 번 더 고르면 버리고 넘어갑니다.";
      leaving = true;
      return;
    }
    show(next);
  });

  save.addEventListener("click", async () => {
    problem.hidden = true;
    const name = current ?? nameInput.value.trim();
    if (current == null && names.includes(name)) {
      problem.textContent = `${name} 스크립트가 이미 있습니다. 목록에서 골라 고치세요.`;
      problem.hidden = false;
      return;
    }
    save.disabled = true;
    try {
      const result = await invoke("save_deploy_script", {
        project: project.name,
        environment: env.name,
        script: name,
        text: editor.value,
      });
      saved = editor.value;
      if (current == null) {
        names = (await invoke("deploy_scripts", { project: project.name, environment: env.name })).names;
        current = name;
        picker.refill(names, current);
        nameField.hidden = true;
        remove.hidden = false;
        whereRows(result.path);
      }
      settle();
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

  // 빼기는 두 번 눌러야 한다. 스크립트는 지우지 않고 보관소로 옮긴다.
  remove.addEventListener("click", async () => {
    if (remove.textContent === "빼기") {
      remove.textContent = "한 번 더 누르면 빼기";
      status.textContent = `${current} 스크립트를 보관소로 옮깁니다.`;
      return;
    }
    problem.hidden = true;
    remove.disabled = true;
    const name = current;
    try {
      const kept = await invoke("remove_deploy_script", { project: project.name, environment: env.name, script: name });
      names = names.filter((n) => n !== name);
      await show(names[0] ?? null);
      status.textContent = `${name} 스크립트를 뺐습니다. ${kept}에 보관했습니다.`;
    } catch (err) {
      problem.textContent = String(err);
      problem.hidden = false;
    } finally {
      remove.disabled = false;
    }
  });

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
  actions.append(problem, remove, shut, save);

  show(names[0] ?? null);

  return [
    heading,
    where,
    scriptField,
    span("detail-subhead", "스크립트가 받는 환경 변수"),
    variables,
    span("pane-note", "배포 창에서 이 환경의 스크립트 하나를 골라 돌립니다. 실패하면(0이 아닌 종료 코드) 배포 실패로 봅니다. `## 제목`으로 시작하는 출력 줄은 배포 로그에서 단계 제목으로 보입니다. 파일을 편집기로 직접 고치거나 같은 디렉토리에 `.sh` 파일을 더해도 됩니다."),
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
      invoke("deploy_scripts", { project: project.name, environment })
        .then((listing) => holder.replaceChildren(...body(project, env, listing, close)))
        .catch((err) => holder.replaceChildren(span("problem", String(err))));
      return [holder];
    },
    { size: "xl", fill: true },
  );
}
