// 프로젝트 탭의 조립 지점 — 목록과 상세가 번갈아 들어선다.

import { span } from "../dom.js";
import { openCreate } from "./create.js";
import { renderDetail } from "./detail.js";
import { renderList } from "./list.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const body = document.getElementById("project-body");

let projects = [];
let errors = [];
/// 상세로 연 프로젝트 이름. 없으면 목록이다.
let opened = null;
/// 방금 만든 프로젝트에서 끝나지 않은 단계. 그 상세를 처음 볼 때 한 번만 보여 준다.
let notices = [];

/// 상세로 간다. 브라우저 기록에 한 칸 남겨 뒤로 가기로 목록에 돌아오게 한다.
function open(name, incomplete = []) {
  if (history.state?.project !== name) history.pushState({ tab: "projects", project: name }, "");
  opened = name;
  notices = incomplete;
  render();
}

function groups() {
  return [...new Set(projects.map((p) => p.group))];
}

/// 새 프로젝트의 기본 상위 디렉토리 — 가장 최근에 만든 프로젝트가 있는 곳.
function recentParent() {
  const latest = [...projects].sort((a, b) => b.created_at.localeCompare(a.created_at))[0];
  return latest ? latest.path.replace(/\/[^/]+$/, "") : "";
}

function create(mode) {
  openCreate(mode, {
    groups: groups(),
    group: groups()[0] ?? "",
    parent: recentParent(),
    onDone: (name, incomplete) => {
      loadProjects().then(() => open(name, incomplete));
    },
  });
}

/// 연 프로젝트 하나만 다시 읽어 그린다. 실패하면 호출한 쪽에 던진다.
async function refresh(name) {
  const fresh = await invoke("project_detail", { name });
  projects = projects.map((p) => (p.name === name ? fresh : p));
  if (opened === name) render();
}

function render() {
  const project = opened && projects.find((p) => p.name === opened);
  if (project) {
    renderDetail(body, project, {
      notices,
      onRefresh: () => refresh(project.name),
      groups: groups(),
      // 이름이 바뀌면 같은 자리(기록)를 새 이름으로 연다. 뒤로 가기가 없는 이름을 가리키지 않게 바꿔 적는다.
      onRenamed: (name) => {
        history.replaceState({ tab: "projects", project: name }, "");
        opened = name;
        loadProjects();
      },
      onUnregistered: () => {
        history.replaceState({ tab: "projects" }, "");
        opened = null;
        loadProjects();
      },
      // 목록에서 들어왔으면 기록을 되돌린다. 그렇지 않으면(기록이 없으면) 목록 칸을 새로 쌓는다.
      onBack: () => {
        if (history.state?.project) history.back();
        else {
          opened = null;
          render();
        }
      },
    });
    return;
  }
  opened = null;
  renderList(body, { projects, errors }, {
    onOpen: open,
    onCreate: create,
  });
}

export async function loadProjects() {
  if (!projects.length) body.replaceChildren(span("list-none", "프로젝트 디렉토리를 읽는 중…"));
  try {
    const listed = await invoke("list_projects");
    projects = listed.projects;
    errors = listed.errors;
  } catch (err) {
    errors = [String(err)];
  }
  render();
}

listen("projects:updated", loadProjects);

// 뒤로 · 앞으로 — 기록에 남은 칸(목록 또는 상세)을 그대로 그린다.
document.addEventListener("app:route", (event) => {
  if (event.detail.tab !== "projects") return;
  opened = event.detail.project ?? null;
  notices = [];
  render();
});

// 왼쪽 메뉴의 [프로젝트]를 누르면 목록으로 돌아간다.
document.addEventListener("app:nav", (event) => {
  if (event.detail !== "projects" || !opened) return;
  opened = null;
  render();
});
