// 프로젝트 상세 — 단계 띠와 로컬 개요.
//
// 이 화면에서 자격 증명은 만들고 등록만 한다. 제거는 자격 증명 화면에서 한다.

import { facts, span } from "../dom.js";
import { openGit } from "./git.js";
import { gitLine } from "./parts.js";

const ROLE_TEXT = {
  example: "예시",
  local: "local",
  other: "규칙 밖 이름",
};

function header(project, onBack) {
  const box = document.createElement("div");
  box.className = "project-head";

  const back = document.createElement("button");
  back.type = "button";
  back.className = "back";
  back.textContent = `← 프로젝트 / ${project.group}`;
  back.addEventListener("click", onBack);

  const line = document.createElement("div");
  line.className = "project-title";
  const h1 = document.createElement("h1");
  h1.className = "mono";
  h1.textContent = project.name;
  line.append(h1, span("path", project.path));
  box.append(back, line);
  return box;
}

/// 단계 칸 하나. 아직 할 수 없는 단계는 이유를 적는다.
function stageCell({ title, state, badge, lines, action }) {
  const cell = document.createElement("div");
  cell.className = `stage-cell ${state}`;
  const top = document.createElement("div");
  top.className = "stage-top";
  top.append(span("stage-title", title), span(`chip ${state === "done" ? "ok" : state === "warn" ? "warn" : ""}`, badge));
  cell.append(top);
  for (const text of lines) cell.append(span("stage-line", text));
  if (action) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = action.label;
    if (action.onClick) {
      button.addEventListener("click", action.onClick);
    } else {
      button.disabled = true;
      button.title = action.reason;
    }
    cell.append(button);
  }
  return cell;
}

/// 이 레포가 어떤 SSH 키로 GitHub 에 접속하는가.
function keyLine(git) {
  if (!git.ssh_key) return "SSH 키: 지정 없음 — 계정 기본 키로 접속합니다";
  // ~/.secrets/keys/github/repo/<소유자>/<레포>/<용도>/key
  const parts = git.ssh_key.split("/");
  const inVault = git.ssh_key.includes("/.secrets/keys/github/");
  return inVault ? `SSH 키: 레포 전용 · ${parts.at(-2)}` : `SSH 키: ${git.ssh_key}`;
}

function stageBand(project) {
  const band = document.createElement("section");
  band.className = "stage-band";
  band.setAttribute("aria-label", "프로젝트 단계");
  const git = project.scan.git;
  const gitDone = project.stages.git === "done";

  band.append(
    stageCell({
      title: "로컬",
      state: project.stages.local,
      badge: project.scan.error ? "찾을 수 없음" : project.stages.local === "warn" ? "주의" : "개발 중",
      lines: project.scan.error ? [project.scan.error] : [project.path],
    }),
    stageCell({
      title: "Git",
      state: gitDone ? "done" : "next",
      badge: gitDone ? "연결됨" : "다음 단계",
      lines: gitDone
        ? [gitLine(git), keyLine(git)]
        : ["원격 레포가 없습니다. 코드를 올릴 준비가 되면 연결합니다."],
      action: project.scan.error
        ? null
        : { label: gitDone ? "키 · 연결 확인…" : "Git 연결…", onClick: () => openGit(project) },
    }),
    stageCell({
      title: "서버",
      state: gitDone ? "next" : "locked",
      badge: gitDone ? "연결 가능" : "Git 연결 후",
      lines: gitDone
        ? ["AWS 콘솔에서 만든 인스턴스를 이 프로젝트의 환경으로 연결합니다."]
        : ["서버는 GitHub에서 코드를 받아 갑니다. Git을 먼저 연결하세요."],
      action: { label: "서버 연결…", reason: gitDone ? "준비 중인 기능입니다." : "Git을 먼저 연결하세요." },
    }),
    stageCell({
      title: "자격 증명",
      state: "open",
      badge: "필요할 때",
      lines: ["IAM · API 키가 필요해지면 환경별로 만들거나 등록합니다."],
      action: { label: "＋ 자격 증명", reason: "준비 중인 기능입니다." },
    }),
  );
  return band;
}

function runtimeFacts(runtimes) {
  if (!runtimes.length) return span("muted", "프로젝트 파일에서 런타임을 감지하지 못했습니다.");
  const list = document.createElement("div");
  list.className = "runtime-list";
  for (const runtime of runtimes) {
    const line = document.createElement("div");
    line.className = "runtime-line";
    const name = runtime.version ? `${runtime.label} ${runtime.version}` : runtime.label;
    line.append(span("strong", name), span("muted", " · 근거 "), span("mono small", runtime.sources.join(" · ")));
    if (runtime.conflict) line.append(span("chip warn", "근거끼리 버전이 다름"));
    list.append(line);
  }
  return list;
}

function localPane(project) {
  const pane = document.createElement("section");
  pane.className = "project-pane";
  const title = document.createElement("h2");
  title.textContent = "로컬";
  const git = project.scan.git;
  const branch = git && git.kind !== "absent"
    ? `${git.branch ?? "분리된 HEAD"} · 커밋 ${git.commits} · 변경 ${git.changes}`
    : "git 저장소가 아닙니다.";
  pane.append(
    title,
    facts([
      ["브랜치", branch, true],
      ["런타임", runtimeFacts(project.scan.runtimes)],
      ["시작", `${project.origin === "created" ? "새로 만듦" : "기존 디렉토리 등록"} · ${project.created_at}`],
    ]),
  );
  return pane;
}

function gitStatus(file) {
  if (file.role === "example") return file.tracked ? "추적됨" : "추적되지 않음";
  if (file.tracked) return "추적됨 — 값이 원격 레포에 올라갈 수 있습니다";
  if (file.ignored === true) return "제외됨";
  if (file.ignored === false) return "추적되지 않음 · .gitignore에 없음";
  return "git 저장소가 아님";
}

function envPane(project) {
  const pane = document.createElement("section");
  pane.className = "project-pane";
  const title = document.createElement("h2");
  title.textContent = "환경 변수 파일";
  pane.append(title, span("pane-note", "로컬 파일이 정본입니다. 서버에는 여기서 반영합니다. 변수 이름만 읽고 값은 읽지 않습니다."));

  const files = project.scan.env_files;
  if (!files.length) {
    pane.append(span("list-none", "환경 변수 파일이 없습니다."));
    return pane;
  }

  const table = document.createElement("table");
  table.className = "env-table";
  const thead = document.createElement("thead");
  const headRow = document.createElement("tr");
  for (const label of ["파일", "환경", "변수", "git"]) {
    const th = document.createElement("th");
    th.textContent = label;
    headRow.append(th);
  }
  thead.append(headRow);
  const tbody = document.createElement("tbody");
  for (const file of files) {
    const tr = document.createElement("tr");
    const env = file.role === "environment" ? file.env : ROLE_TEXT[file.role];
    const cells = [
      span("mono", file.name),
      span(file.role === "example" ? "muted" : "strong", env),
      span("", String(file.variables)),
      span(file.exposed ? "warn-text" : "muted", gitStatus(file)),
    ];
    for (const node of cells) {
      const td = document.createElement("td");
      td.append(node);
      tr.append(td);
    }
    tbody.append(tr);
  }
  table.append(thead, tbody);
  pane.append(table);

  const exposed = files.filter((f) => f.exposed);
  if (exposed.length) {
    pane.append(
      span(
        "notice warn",
        `${exposed.map((f) => f.name).join(", ")}의 값이 원격 레포에 올라갈 수 있습니다. Git을 연결하기 전에 .gitignore에 추가하세요.`,
      ),
    );
  }
  return pane;
}

export function renderDetail(mount, project, { onBack, notices }) {
  const nodes = [header(project, onBack)];
  for (const message of notices) nodes.push(span("notice warn", message));
  nodes.push(stageBand(project));

  const body = document.createElement("div");
  body.className = "project-body-grid";
  if (project.scan.error) {
    body.append(span("problem", `${project.scan.error} 디렉토리를 옮겼다면 기록의 경로와 맞지 않는 상태입니다.`));
  } else {
    body.append(localPane(project), envPane(project));
  }
  nodes.push(body);
  mount.replaceChildren(...nodes);
}
