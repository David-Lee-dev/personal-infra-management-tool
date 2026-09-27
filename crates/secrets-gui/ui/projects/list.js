// 프로젝트 목록 — 그룹별로 묶은 카드. 카드를 누르면 상세로 들어간다.

import { span } from "../dom.js";
import { nextStep, stageTrack } from "./parts.js";

function head(handlers) {
  const bar = document.createElement("div");
  bar.className = "project-bar";
  const title = document.createElement("h1");
  title.textContent = "프로젝트";
  const gap = span("inner-tabs-gap", "");
  const register = document.createElement("button");
  register.type = "button";
  register.textContent = "기존 디렉토리 등록";
  register.addEventListener("click", () => handlers.onCreate("register"));
  const create = document.createElement("button");
  create.type = "button";
  create.className = "primary";
  create.textContent = "＋ 새 프로젝트";
  create.addEventListener("click", () => handlers.onCreate("new"));
  bar.append(title, gap, register, create);
  return bar;
}

/// 로컬 레포의 원격(origin).
function repoText(git) {
  if (!git || git.kind === "absent") return "git 저장소 아님";
  return git.kind === "remote" ? git.repo ?? git.origin : "원격 없음 (로컬 git만)";
}

/// 카드 한 장. 이름 · 경로 · 단계 · 레포 · 서버 환경 · 다음 할 일.
function card(project, onOpen) {
  const box = document.createElement("button");
  box.type = "button";
  box.className = "project-card";
  box.addEventListener("click", () => onOpen(project.name));

  const top = document.createElement("div");
  top.className = "project-card-top";
  top.append(span("mono strong project-card-name", project.name), stageTrack(project.stages));

  const facts = document.createElement("dl");
  facts.className = "fact-list";
  // 서버는 연결된 환경마다 한 줄 — `환경: 서버 (계정)`. 이름 칸은 첫 줄에만 적는다.
  const rows = [["레포", repoText(project.scan.git)]];
  project.environments.forEach((e, i) => {
    rows.push([i === 0 ? "서버" : "", `${e.name}: ${e.server_name} (${e.login} 계정)`]);
  });
  for (const [label, value] of rows) {
    const dt = document.createElement("dt");
    dt.textContent = label;
    const dd = document.createElement("dd");
    dd.textContent = value;
    facts.append(dt, dd);
  }

  box.append(top, span("path project-card-path", project.path), facts);
  const next = nextStep(project);
  if (next.tone !== "none") box.append(span(`next ${next.tone}`, `다음 할 일 · ${next.text}`));
  return box;
}

/// 프로젝트가 하나도 없을 때. 화면 가운데에서 시작할 방법 두 가지를 보여 준다.
function emptyState(handlers) {
  const box = document.createElement("div");
  box.className = "project-empty";
  const title = document.createElement("h1");
  title.textContent = "아직 프로젝트가 없습니다";
  const note = span("project-empty-note", "새 디렉토리에서 시작하거나, 작업 중인 디렉토리를 등록하세요.");
  const buttons = document.createElement("div");
  buttons.className = "project-empty-actions";
  const create = document.createElement("button");
  create.type = "button";
  create.className = "primary";
  create.textContent = "＋ 새 프로젝트";
  create.addEventListener("click", () => handlers.onCreate("new"));
  const register = document.createElement("button");
  register.type = "button";
  register.textContent = "기존 디렉토리 등록";
  register.addEventListener("click", () => handlers.onCreate("register"));
  buttons.append(create, register);
  box.append(title, note, buttons);
  return box;
}

export function renderList(mount, { projects, errors }, handlers) {
  if (!projects.length && !errors.length) {
    mount.replaceChildren(emptyState(handlers));
    return;
  }

  const scroll = document.createElement("div");
  scroll.className = "project-scroll";
  for (const message of errors) scroll.append(span("problem", message));
  const groups = [...new Set(projects.map((p) => p.group))];
  for (const group of groups) {
    const section = document.createElement("section");
    section.className = "project-group";
    const title = document.createElement("h2");
    title.textContent = group;
    const grid = document.createElement("div");
    grid.className = "project-cards";
    for (const project of projects.filter((p) => p.group === group)) grid.append(card(project, handlers.onOpen));
    section.append(title, grid);
    scroll.append(section);
  }
  mount.replaceChildren(head(handlers), scroll);
}
