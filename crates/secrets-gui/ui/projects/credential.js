// 자격 증명 연결 — 이미 있는 자격 증명을 이 프로젝트의 어느 파일에 넣었는지 기록한다.
//
// 프로젝트는 자격 증명을 만들지도 지우지도 않는다(발급 · 제거는 자격 증명 화면). 파일에 값을 넣는
// 일도 사람이 한다 — 여기서는 소비처 기록만 남긴다. 기록은 자격 증명 쪽에 쌓인다.

import { span } from "../dom.js";
import { modal } from "../modal.js";

const { invoke } = window.__TAURI__.core;

function field(label, control, help) {
  const box = document.createElement("div");
  box.className = "field";
  const el = document.createElement("label");
  el.textContent = label;
  box.append(el, control);
  if (help) box.append(help);
  return box;
}

function select(options, empty) {
  const el = document.createElement("select");
  if (!options.length) {
    const option = document.createElement("option");
    option.value = "";
    option.textContent = empty;
    el.append(option);
    el.disabled = true;
  }
  for (const [value, label] of options) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    el.append(option);
  }
  return el;
}

function choiceLabel(c) {
  return c.purpose ? c.name + " — " + c.purpose : c.name;
}

/// 연결 창. `onDone` 은 기록한 뒤 부른다.
export function openCredentialLink(project, choices, onDone) {
  modal(`자격 증명 연결 · ${project.name}`, (close) => {
    let kind = "iam";

    const kindBox = document.createElement("div");
    kindBox.className = "choice-inline";
    for (const [value, label] of [["iam", "AWS IAM"], ["etc", "기타 항목 (파일 · 값)"]]) {
      const row = document.createElement("label");
      row.className = "choice-row";
      const radio = document.createElement("input");
      radio.type = "radio";
      radio.name = "credential-kind";
      radio.checked = value === kind;
      radio.addEventListener("change", () => {
        kind = value;
        show();
      });
      row.append(radio, span("", label));
      kindBox.append(row);
    }

    const iam = select(
      choices.iams.map((c) => [c.owner + "|" + c.name, choiceLabel(c)]),
      "등록된 IAM이 없습니다",
    );
    const etc = select(
      choices.etcs.map((c) => [c.owner + "|" + c.name, c.owner + "/" + choiceLabel(c)]),
      "등록된 기타 항목이 없습니다",
    );

    // IAM 은 프로젝트 뿌리의 환경 변수 파일에, 기타 항목은 프로젝트 안의 아무 경로에 넣는다.
    const envFiles = project.scan.env_files.filter((f) => f.role !== "example");
    const envFile = select(
      envFiles.map((f) => [f.name, f.env ? f.name + " · " + f.env : f.name]),
      "환경 변수 파일이 없습니다",
    );
    const variable = document.createElement("input");
    variable.type = "text";
    variable.className = "mono";
    variable.placeholder = "환경 변수 이름";
    variable.spellcheck = false;
    const secretHint = span("field-help", "");
    variable.addEventListener("input", () => {
      const id = variable.value.trim();
      secretHint.textContent = id.endsWith("ACCESS_KEY_ID")
        ? "시크릿 변수: " + id.slice(0, -"ACCESS_KEY_ID".length) + "SECRET_ACCESS_KEY"
        : "…ACCESS_KEY_ID로 끝나는 이름이어야 합니다.";
    });
    const etcPath = document.createElement("input");
    etcPath.type = "text";
    etcPath.className = "mono";
    etcPath.placeholder = "프로젝트 안의 대상 파일 경로";
    etcPath.spellcheck = false;

    const iamFields = [field("IAM", iam), field("환경 변수 파일", envFile), field("키 ID 변수", variable, secretHint)];
    const etcFields = [field("기타 항목", etc), field("프로젝트 안의 경로", etcPath)];
    function show() {
      for (const f of iamFields) f.hidden = kind !== "iam";
      for (const f of etcFields) f.hidden = kind !== "etc";
    }
    show();

    const problem = span("problem", "");
    problem.hidden = true;
    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.textContent = "취소";
    cancel.addEventListener("click", close);
    const save = document.createElement("button");
    save.type = "button";
    save.className = "primary";
    save.textContent = "기록";
    save.addEventListener("click", async () => {
      problem.hidden = true;
      save.disabled = true;
      const root = project.path.replace(/\/$/, "");
      // 같은 자리를 두 번 적지 않는다. 자격 증명 쪽은 적힌 글자 그대로 견주므로 `~/…` 로 적힌
      // 기록과 절대 경로를 다른 자리로 본다 — 여기서 프로젝트 안의 경로로 한 번 더 견준다.
      const taken = (match) => choices.linked.find((c) => !c.host && match(c));
      try {
        if (kind === "iam") {
          const [account, name] = iam.value.split("|");
          const id = variable.value.trim();
          const same = taken((c) => c.kind === "iam" && c.file === envFile.value && c.variable === id);
          if (same) throw new Error(`${envFile.value}의 ${id}은(는) 이미 ${same.name}(으)로 기록되어 있습니다.`);
          await invoke("add_iam_consumer", {
            at: { account, name },
            place: { host: "local", file: root + "/" + envFile.value, id_variable: id },
          });
        } else {
          const [group, name] = etc.value.split("|");
          const inner = etcPath.value.trim().replace(/^\/+/, "");
          if (!inner) throw new Error("프로젝트 안의 경로를 입력하세요.");
          if (taken((c) => c.kind === "etc" && c.name === group + "/" + name && c.file === inner)) {
            throw new Error(`${inner}은(는) 이미 ${group}/${name}(으)로 기록되어 있습니다.`);
          }
          await invoke("add_etc_consumer", {
            at: { project: group, name },
            place: { host: "local", file: root + "/" + inner },
          });
        }
        close();
        onDone();
      } catch (err) {
        problem.textContent = err instanceof Error ? err.message : String(err);
        problem.hidden = false;
        save.disabled = false;
      }
    });
    const actions = document.createElement("div");
    actions.className = "modal-actions";
    actions.append(problem, cancel, save);

    return [
      span("pane-note", "이미 있는 자격 증명을 이 프로젝트의 어느 파일에 넣었는지 기록합니다. 파일에 값을 넣는 일은 직접 합니다 — 값은 자격 증명 화면에서 복사할 수 있습니다. 발급과 제거는 자격 증명 화면에서 합니다."),
      field("종류", kindBox),
      ...iamFields,
      ...etcFields,
      actions,
    ];
  });
}
