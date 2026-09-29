// 환경 변수 — 로컬 뿌리의 파일 하나를 서버 배포 경로 뿌리의 파일 하나로 올린다.
//
// 서버 쪽 이름은 런타임이 읽는 이름이라 사용자가 고른다(.env · .env.local …). 자리는 늘 뿌리다.
// 로컬이 정본이다. 비교는 양쪽 해시로만 하고 값은 옮기지 않는다. 화면에는 변수 이름만 나온다.
// 서버에 쓰는 일이라 무엇이 바뀌는지 먼저 보여 주고 한 번 더 확인을 받는다.

import { pickOrType, span } from "../dom.js";
import { modal } from "../modal.js";
import { invoke } from "../ipc.js";

const TRACKING_NOTE = {
  tracked: "서버 레포가 이 파일을 추적합니다. 코드를 받을 때 덮어쓰이거나 충돌합니다. 레포에서 빼고 .gitignore에 넣으세요.",
  unignored: "서버 레포의 .gitignore가 이 파일을 제외하지 않습니다. git status에 나오고 실수로 커밋될 수 있습니다.",
};

function field(label, control) {
  const box = document.createElement("div");
  box.className = "field";
  const el = document.createElement("label");
  el.textContent = label;
  box.append(el, control);
  return box;
}

function names(list) {
  return list.join(", ");
}

/// 비교 결과를 줄로 그린다. 반영할 필요가 있으면 true.
function describe(found, box) {
  const lines = [];
  const line = (label, text, tone = "") => {
    const row = document.createElement("div");
    row.className = "detected-line";
    row.append(span("muted", label), span(tone, text));
    lines.push(row);
  };
  let needed = false;
  // 코드를 받기 전에는 올리지 않는다 — 먼저 놓인 파일이 코드 받기를 막는다.
  if (found.state === "no_directory") {
    line("결과", "서버에 배포 경로가 없습니다. [코드 받기]를 먼저 하세요.", "warn-text");
  } else if (found.tracking === "none") {
    line("결과", "배포 경로에 아직 코드가 없습니다(git 저장소가 아님). [코드 받기]를 먼저 하세요.", "warn-text");
  } else if (found.state === "same") {
    line("결과", "일치합니다. 파일 내용이 바이트까지 같습니다.", "strong");
  } else if (found.state === "server_missing") {
    line("결과", "서버에 이 파일이 없습니다.", "warn-text");
    needed = true;
  } else {
    needed = true;
    line("결과", "다릅니다.", "warn-text");
    if (found.local_only.length) line("서버에 새로 생김", names(found.local_only));
    if (found.changed.length) line("값이 바뀜", names(found.changed));
    if (found.server_only.length) line("서버에서 사라짐", names(found.server_only), "warn-text");
    if (!found.local_only.length && !found.changed.length && !found.server_only.length) {
      line("차이", "변수와 값은 같습니다. 주석 · 빈 줄 · 순서가 다르거나 여러 줄 값이 다릅니다.");
    }
  }
  if (found.mode || found.owner) {
    const mode = found.mode ? "권한 " + found.mode : "";
    const owner = found.owner ? "소유자 " + found.owner : "";
    line("서버 파일", [mode, owner].filter(Boolean).join(" · "), found.mode && found.mode !== "600" ? "warn-text" : "");
  }
  box.replaceChildren(...lines);
  const note = TRACKING_NOTE[found.tracking];
  if (note) box.append(span("notice warn", note));
  return needed;
}

function body(project, env, close) {
  const files = project.scan.env_files.filter((f) => f.role !== "example");
  let chosen = env.env_file ?? "";

  const select = document.createElement("select");
  const none = document.createElement("option");
  none.value = "";
  none.textContent = "고르지 않음";
  select.append(none);
  for (const f of files) {
    const option = document.createElement("option");
    option.value = f.name;
    option.textContent = f.name + " · 변수 " + f.variables + "개";
    select.append(option);
  }
  // 기록된 파일이 뿌리에서 사라졌어도 무엇을 골랐었는지는 보이게 한다.
  if (chosen && !files.some((f) => f.name === chosen)) {
    const gone = document.createElement("option");
    gone.value = chosen;
    gone.textContent = chosen + " · 로컬에 없음";
    select.append(gone);
  }
  select.value = chosen;

  // 서버 쪽 이름. 목록은 흔한 이름일 뿐이고 무엇이든 직접 입력할 수 있다.
  const serverNames = [...new Set([".env", ".env.local", env.server_env_file])];
  const serverPick = pickOrType(serverNames, { selected: env.server_env_file, placeholder: "서버에 저장할 파일 이름" });

  const summary = document.createElement("dl");
  summary.className = "git-summary";
  for (const [label, value] of [
    ["서버 자리", `${env.path}/ (배포 경로 뿌리)`],
    ["서버", `${env.server_name} · ${env.login}@${env.address}`],
    ["방향", "로컬 → 서버. 로컬 파일이 정본입니다."],
  ]) {
    const dt = document.createElement("dt");
    dt.textContent = label;
    const dd = document.createElement("dd");
    dd.textContent = value;
    summary.append(dt, dd);
  }

  const result = document.createElement("div");
  result.className = "detected";
  const confirmBox = document.createElement("div");
  confirmBox.className = "detected";

  const problem = span("problem", "");
  problem.hidden = true;
  const compare = document.createElement("button");
  compare.type = "button";
  compare.textContent = "다시 비교";
  const push = document.createElement("button");
  push.type = "button";
  push.className = "primary";
  push.textContent = "서버에 반영";
  push.disabled = true;
  let last = null;

  function showProblem(err) {
    problem.textContent = String(err);
    problem.hidden = false;
  }

  async function run() {
    problem.hidden = true;
    confirmBox.replaceChildren();
    push.disabled = true;
    push.textContent = "서버에 반영";
    if (!chosen) {
      result.replaceChildren(span("muted", "올릴 로컬 파일을 고르면 서버 파일과 비교합니다."));
      compare.disabled = true;
      return;
    }
    compare.disabled = true;
    result.replaceChildren(span("muted", "서버 파일과 비교하는 중… 값은 옮기지 않고 해시만 비교합니다."));
    try {
      last = await invoke("compare_env", { project: project.name, environment: env.name });
      push.disabled = !describe(last, result);
    } catch (err) {
      result.replaceChildren();
      showProblem(err);
    } finally {
      compare.disabled = false;
    }
  }

  let savedServer = env.server_env_file;
  async function save() {
    const next = select.value || null;
    const serverFile = serverPick.value().trim();
    if (!serverFile) return;
    if ((next ?? "") === chosen && serverFile === savedServer) return;
    select.disabled = true;
    try {
      await invoke("choose_env_file", { project: project.name, environment: env.name, file: next, serverFile });
      chosen = next ?? "";
      savedServer = serverFile;
      await run();
    } catch (err) {
      select.value = chosen;
      serverPick.set(savedServer);
      showProblem(err);
    } finally {
      select.disabled = false;
    }
  }
  select.addEventListener("change", save);
  // 목록에서 고르면 바로, 직접 입력은 칸을 벗어나거나 Enter 를 누를 때 저장한다.
  serverPick.node.addEventListener("change", save);
  compare.addEventListener("click", run);

  // 첫 누름은 무엇이 바뀌는지 보여 주고, 두 번째 누름이 서버에 쓴다.
  push.addEventListener("click", async () => {
    if (!confirmBox.childElementCount) {
      const gone = last?.server_only ?? [];
      confirmBox.append(
        span("strong", `서버의 ${last.server_file}을(를) 로컬 ${chosen} 내용으로 바꿉니다.`),
        span("muted small", "권한은 600으로 둡니다. 서버의 이전 내용은 남기지 않습니다."),
      );
      if (gone.length) confirmBox.append(span("warn-text", "서버에서 사라지는 변수: " + names(gone)));
      push.textContent = "반영";
      return;
    }
    problem.hidden = true;
    push.disabled = true;
    compare.disabled = true;
    select.disabled = true;
    push.textContent = "반영하는 중…";
    try {
      last = await invoke("push_env", { project: project.name, environment: env.name });
      confirmBox.replaceChildren(span("strong", "반영했습니다. 다시 비교해 일치를 확인했습니다."));
      describe(last, result);
      push.textContent = "서버에 반영";
    } catch (err) {
      showProblem(err);
      push.textContent = "반영";
      push.disabled = false;
    } finally {
      compare.disabled = false;
      select.disabled = false;
    }
  });

  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.textContent = "닫기";
  cancel.addEventListener("click", close);
  const actions = document.createElement("div");
  actions.className = "modal-actions";
  actions.append(problem, cancel, compare, push);

  run();
  return [
    field("올릴 로컬 파일", select),
    field("서버에 둘 이름", serverPick.node),
    summary,
    result,
    confirmBox,
    span("pane-note", "값은 화면 · 명령 인자 · 작업 로그에 나오지 않습니다. 비교는 이번 한 번만 쓰는 임의 값을 섞은 해시로 합니다."),
    actions,
  ];
}

/// 환경 하나의 환경 변수 창을 연다.
export function openEnv(project, environment) {
  const env = project.environments.find((e) => e.name === environment);
  modal(`환경 변수 · ${project.name} · ${environment}`, (close) => {
    const holder = document.createElement("div");
    holder.className = "git-form";
    holder.append(...body(project, env, close));
    return [holder];
  });
}
