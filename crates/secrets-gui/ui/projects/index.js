// 프로젝트 탭의 조립 지점 — 목록과 상세가 번갈아 들어선다.

import { span } from "../dom.js";
import { openCreate } from "./create.js";
import { renderDetail } from "./detail.js";
import { renderList } from "./list.js";
import { renderSide } from "./side.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const body = document.getElementById("project-body");
const side = document.getElementById("side-projects");

let projects = [];
let errors = [];
let filter = "all";
/// 상세로 연 프로젝트 이름. 없으면 목록이다.
let opened = null;
/// 방금 만든 프로젝트에서 끝나지 않은 단계. 그 상세를 처음 볼 때 한 번만 보여 준다.
let notices = [];

/// 다른 화면에 있을 때 왼쪽 목록에서 프로젝트를 누르면 프로젝트 탭으로 옮긴다.
/// 탭 전환은 tabs.js 의 몫이라 이벤트로 부탁한다.
function showProjectsTab() {
  document.dispatchEvent(new CustomEvent("app:show-tab", { detail: "projects" }));
}

function open(name) {
  opened = name;
  notices = [];
  showProjectsTab();
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
      opened = name;
      notices = incomplete;
      loadProjects();
    },
  });
}

function render() {
  renderSide(side, projects, opened, open);
  const project = opened && projects.find((p) => p.name === opened);
  if (project) {
    renderDetail(body, project, {
      notices,
      onBack: () => {
        opened = null;
        render();
      },
    });
    return;
  }
  opened = null;
  renderList(body, { projects, errors, filter }, {
    onOpen: open,
    onCreate: create,
    onFilter: (next) => {
      filter = next;
      render();
    },
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
