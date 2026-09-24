// 프로젝트 목록 — 그룹별로 묶고, 단계와 다음 할 일을 한 줄에 보여 준다.

import { span } from "../dom.js";
import { gitLine, nextStep, runtimeLine, stageTrack } from "./parts.js";

const FILTERS = [
  { id: "all", label: "전체", test: () => true },
  { id: "local", label: "로컬만", test: (p) => p.stages.git !== "done" },
  { id: "git", label: "Git까지", test: (p) => p.stages.git === "done" && p.stages.server !== "done" },
  { id: "warn", label: "주의 필요", test: (p) => nextStep(p).tone === "warn" },
];

function head(projects, handlers) {
  const bar = document.createElement("div");
  bar.className = "project-bar";
  const title = document.createElement("h1");
  title.textContent = "프로젝트";
  const count = span("project-count", `${projects.length}개`);
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
  bar.append(title, count, gap, register, create);
  return bar;
}

function filters(projects, current, onFilter) {
  const box = document.createElement("div");
  box.className = "project-filters";
  for (const filter of FILTERS) {
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "filter-chip";
    chip.setAttribute("aria-pressed", String(filter.id === current));
    chip.textContent = `${filter.label} ${projects.filter(filter.test).length}`;
    chip.addEventListener("click", () => onFilter(filter.id));
    box.append(chip);
  }
  return box;
}

function nameCell(project) {
  const box = document.createElement("div");
  box.className = "project-name";
  box.append(span("mono strong", project.name), span("path", project.path));
  return box;
}

function projectRow(project, onOpen) {
  const tr = document.createElement("tr");
  tr.className = "project-row";
  tr.tabIndex = 0;
  const next = nextStep(project);
  const cells = [
    nameCell(project),
    stageTrack(project.stages),
    span("mono small", gitLine(project.scan.git)),
    span("small", project.scan.error ? "—" : runtimeLine(project.scan.runtimes)),
    span(`next ${next.tone}`, next.text),
  ];
  for (const node of cells) {
    const td = document.createElement("td");
    td.append(node);
    tr.append(td);
  }
  tr.addEventListener("click", () => onOpen(project.name));
  tr.addEventListener("keydown", (event) => {
    if (event.key === "Enter") onOpen(project.name);
  });
  return tr;
}

function table(projects, onOpen) {
  const el = document.createElement("table");
  el.className = "project-table";
  const colgroup = document.createElement("colgroup");
  for (const width of ["27%", "17%", "24%", "17%", "15%"]) {
    const col = document.createElement("col");
    col.style.width = width;
    colgroup.append(col);
  }
  const thead = document.createElement("thead");
  const tr = document.createElement("tr");
  for (const label of ["프로젝트", "단계", "Git", "런타임", "다음 할 일"]) {
    const th = document.createElement("th");
    th.textContent = label;
    tr.append(th);
  }
  thead.append(tr);

  const tbody = document.createElement("tbody");
  const groups = [...new Set(projects.map((p) => p.group))];
  for (const group of groups) {
    const groupRow = document.createElement("tr");
    groupRow.className = "group-row";
    const td = document.createElement("td");
    td.colSpan = 5;
    td.textContent = group;
    groupRow.append(td);
    tbody.append(groupRow);
    for (const project of projects.filter((p) => p.group === group)) {
      tbody.append(projectRow(project, onOpen));
    }
  }
  el.append(colgroup, thead, tbody);
  return el;
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

export function renderList(mount, { projects, errors, filter }, handlers) {
  if (!projects.length && !errors.length) {
    mount.replaceChildren(emptyState(handlers));
    return;
  }

  const nodes = [head(projects, handlers)];

  nodes.push(filters(projects, filter, handlers.onFilter));
  for (const message of errors) nodes.push(span("problem", message));

  const test = FILTERS.find((f) => f.id === filter)?.test ?? (() => true);
  const shown = projects.filter(test);
  const scroll = document.createElement("div");
  scroll.className = "project-scroll";
  scroll.append(shown.length ? table(shown, handlers.onOpen) : span("list-none", "조건에 맞는 프로젝트가 없습니다."));
  nodes.push(scroll);
  mount.replaceChildren(...nodes);
}
