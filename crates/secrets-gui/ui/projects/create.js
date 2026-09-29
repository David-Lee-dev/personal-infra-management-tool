// 새 프로젝트 — 빈 디렉토리를 만들거나, 작업 중인 디렉토리를 등록한다.
//
// 만들기는 디렉토리 생성과 `git init` 만 한다. 등록은 디렉토리를 바꾸지 않는다.
// GitHub · AWS · 서버에는 아무것도 만들지 않는다.

import { pickOrType, span } from "../dom.js";
import { modal } from "../modal.js";
import { gitLine, runtimeLine } from "./parts.js";
import { invoke } from "../ipc.js";

const STATE_TEXT = {
  missing: ["비어 있는 경로", "ok"],
  empty: ["빈 디렉토리 — 그대로 씁니다", "ok"],
  occupied: ["이미 파일이 있는 디렉토리", "warn"],
  not_directory: ["디렉토리가 아닌 파일이 있습니다", "warn"],
};

function field(label, input, help, labelFor = input.id) {
  const box = document.createElement("div");
  box.className = "field";
  const el = document.createElement("label");
  el.textContent = label;
  const control = input.matches("input, select") ? input : input.querySelector("select, input");
  if (control && !control.id) control.id = `field-${crypto.randomUUID()}`;
  el.htmlFor = labelFor || control?.id || "";
  box.append(el, input);
  if (help) box.append(help);
  return box;
}

function textInput(id, value = "", { mono = false, placeholder = "" } = {}) {
  const input = document.createElement("input");
  input.id = id;
  input.type = "text";
  input.value = value;
  input.placeholder = placeholder;
  input.spellcheck = false;
  input.autocomplete = "off";
  if (mono) input.className = "mono";
  return input;
}

/// 그룹 — 있는 그룹은 드롭다운으로 고르고, 새 그룹은 "＋ 새 그룹"에서 입력한다.
function groupInput(groups, value) {
  return pickOrType(groups, { newLabel: "＋ 새 그룹", placeholder: "그룹 이름", selected: value });
}

/// 입력이 멈춘 뒤에 한 번만 부른다. 타자마다 디스크를 읽지 않게.
function debounce(fn, ms = 250) {
  let timer;
  return (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
}

/// 입력칸 옆 "Finder에서 선택". ~/workspace 안에서만 고른다. 고르면 입력칸을 채우고 `onPicked` 를 부른다.
function withPicker(input, title, onPicked) {
  const row = document.createElement("div");
  row.className = "with-picker";
  const pick = document.createElement("button");
  pick.type = "button";
  pick.textContent = "Finder에서 선택";
  const problem = span("problem small", "");
  problem.hidden = true;
  pick.addEventListener("click", async () => {
    problem.hidden = true;
    try {
      const chosen = await invoke("pick_project_folder", { title });
      if (!chosen) return;
      input.value = chosen;
      onPicked();
    } catch (err) {
      problem.textContent = String(err);
      problem.hidden = false;
    }
  });
  row.append(input, pick);
  row.id = `${input.id}-row`;
  const box = document.createElement("div");
  box.className = "picker-box";
  box.append(row, problem);
  return box;
}

function modeSwitch(current, onChange) {
  const box = document.createElement("div");
  box.className = "segmented";
  box.setAttribute("role", "tablist");
  for (const [id, label] of [["new", "새 디렉토리 만들기"], ["register", "기존 디렉토리 등록"]]) {
    const button = document.createElement("button");
    button.type = "button";
    button.setAttribute("role", "tab");
    button.setAttribute("aria-selected", String(id === current));
    button.textContent = label;
    button.addEventListener("click", () => onChange(id));
    box.append(button);
  }
  return box;
}

function actions(submitLabel, onSubmit, close) {
  const row = document.createElement("div");
  row.className = "modal-actions";
  const problem = span("problem", "");
  problem.hidden = true;
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.textContent = "취소";
  cancel.addEventListener("click", close);
  const submit = document.createElement("button");
  submit.type = "button";
  submit.className = "primary";
  submit.textContent = submitLabel;
  submit.addEventListener("click", async () => {
    problem.hidden = true;
    submit.disabled = true;
    try {
      await onSubmit();
    } catch (err) {
      problem.textContent = String(err);
      problem.hidden = false;
    } finally {
      submit.disabled = false;
    }
  });
  row.append(problem, cancel, submit);
  return { row, submit };
}

/* ── 새 디렉토리 ────────────────────────────────────── */

function newForm(context, close) {
  const name = textInput("np-name", "", { mono: true, placeholder: "영문 · 숫자 · - · _" });
  const parent = textInput("np-parent", context.parent, { mono: true, placeholder: "프로젝트를 만들 상위 폴더" });
  const directory = textInput("np-dir", "", { mono: true });
  const group = groupInput(context.groups, context.group);
  const git = document.createElement("input");
  git.type = "checkbox";
  git.id = "np-git";
  git.checked = true;

  const target = span("target-path mono", "—");
  const targetState = span("chip", "");
  targetState.hidden = true;
  const refreshTarget = debounce(async () => {
    targetState.hidden = true;
    if (!parent.value.trim() || !directory.value.trim()) {
      target.textContent = "—";
      return;
    }
    try {
      const check = await invoke(
        "inspect_project_path",
        { path: `${parent.value.trim().replace(/\/$/, "")}/${directory.value.trim()}` },
        { quiet: true },
      );
      target.textContent = check.path;
      const [text, tone] = STATE_TEXT[check.state];
      targetState.textContent = text;
      targetState.className = `chip ${tone}`;
      targetState.hidden = false;
    } catch (err) {
      target.textContent = String(err);
    }
  });
  parent.addEventListener("input", refreshTarget);
  directory.addEventListener("input", refreshTarget);

  const targetLine = document.createElement("div");
  targetLine.className = "target-line";
  targetLine.append(target, targetState);

  const gitLabel = document.createElement("label");
  gitLabel.className = "check-line";
  gitLabel.append(git, " Git 저장소 초기화 (초기 브랜치: main)");

  const effects = span(
    "pane-note",
    "선택한 상위 폴더에 프로젝트 디렉토리를 만듭니다.",
  );

  const { row } = actions("프로젝트 만들기", async () => {
    const created = await invoke("create_project", {
      form: {
        name: name.value,
        group: group.value(),
        parent: parent.value,
        directory: directory.value,
        init_git: git.checked,
      },
    });
    close();
    context.onDone(created.project.name, created.incomplete);
  }, close);

  return [
    field("이름", name),
    field("상위 디렉토리", withPicker(parent, "프로젝트를 만들 상위 디렉토리", refreshTarget), null, parent.id),
    field("디렉토리 이름", directory),
    field("만들 경로", targetLine),
    field("그룹", group.node, null, ""),
    gitLabel,
    effects,
    row,
  ];
}

/* ── 기존 디렉토리 ──────────────────────────────────── */

function detected(check) {
  const box = document.createElement("div");
  box.className = "detected";
  if (check.project) {
    box.append(span("problem", `이미 프로젝트 ${check.project}(으)로 등록된 경로입니다.`));
    return box;
  }
  if (check.state === "missing" || check.state === "not_directory") {
    box.append(span("problem", check.state === "missing" ? "디렉토리를 찾을 수 없습니다." : "디렉토리가 아닙니다."));
    return box;
  }
  const scan = check.scan;
  const envs = scan.env_files.map((f) => `${f.name} (${f.variables})`).join(" · ") || "없음";
  const lines = [
    ["Git", gitLine(scan.git)],
    ["런타임", runtimeLine(scan.runtimes)],
    ["환경 변수 파일", envs],
  ];
  for (const [label, value] of lines) {
    const line = document.createElement("div");
    line.className = "detected-line";
    line.append(span("muted", label), span("mono small", value));
    box.append(line);
  }
  return box;
}

function registerForm(context, close) {
  const path = textInput("np-path", "", { mono: true, placeholder: "등록할 프로젝트 폴더 경로" });
  const name = textInput("np-name", "", { mono: true, placeholder: "영문 · 숫자 · - · _" });
  const group = groupInput(context.groups, context.group);
  const found = document.createElement("div");
  found.className = "detected";

  const inspect = debounce(async () => {
    if (!path.value.trim()) {
      found.replaceChildren();
      return;
    }
    try {
      const check = await invoke("inspect_project_path", { path: path.value }, { quiet: true });
      found.replaceChildren(...detected(check).childNodes);
    } catch (err) {
      found.replaceChildren(span("problem", String(err)));
    }
  }, 350);
  path.addEventListener("input", inspect);

  const { row } = actions("등록", async () => {
    const project = await invoke("register_project", {
      form: { name: name.value, group: group.value(), path: path.value },
    });
    close();
    context.onDone(project.name, []);
  }, close);

  return [
    field(
      "디렉토리 경로",
      withPicker(path, "등록할 프로젝트 디렉토리", inspect),
      span("field-help", "~/workspace 안의 프로젝트 폴더를 선택하세요."),
      path.id,
    ),
    found,
    field("이름", name),
    field("그룹", group.node, null, ""),
    row,
  ];
}

/// 새 프로젝트 창을 연다. `mode` 는 "new" 또는 "register".
export function openCreate(mode, context) {
  modal("새 프로젝트", (close) => {
    const holder = document.createElement("div");
    holder.className = "create-form";
    function show(next) {
      const form = next === "new" ? newForm(context, close) : registerForm(context, close);
      holder.replaceChildren(modeSwitch(next, show), ...form);
      holder.querySelector("input")?.focus();
    }
    show(mode);
    return [holder];
  });
}
