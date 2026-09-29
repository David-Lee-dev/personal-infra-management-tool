// 서버 탭의 조립 지점 — 목록과 상세가 번갈아 들어선다.
//
// 서버는 접속할 수 있는 기계 하나다. AWS 인지 로컬 기기인지는 속성일 뿐이고, 계정을 누르면
// Ghostty 새 창에서 그 계정으로 들어간다.

import { span } from "../dom.js";
import { showTab } from "../tabs.js";
import { termWrite } from "../terminal.js";
import { renderDetail } from "./detail.js";
import { renderList } from "./list.js";
import { invoke } from "../ipc.js";

const { listen } = window.__TAURI__.event;

const body = document.getElementById("server-body");

let listed = { servers: [], errors: [], suggestions: 0, groups: [] };
/// 상세로 연 서버 id. 없으면 목록이다.
let opened = null;

/// 다른 화면(프로젝트 · 자격 증명)에서 서버 상세로 간다. 뒤로 가기로 돌아올 수 있게 기록을 남긴다.
export function openServer(id) {
  history.pushState({ tab: "servers", project: null, server: id }, "");
  showTab("servers");
  opened = id;
  render();
}

/// 다른 화면에서 서버 목록으로 간다.
export function showServers() {
  history.pushState({ tab: "servers", project: null, server: null }, "");
  showTab("servers");
  opened = null;
  render();
}

function open(id) {
  if (history.state?.server !== id) history.pushState({ tab: "servers", project: null, server: id }, "");
  opened = id;
  render();
}

function render() {
  const server = opened && listed.servers.find((s) => s.id === opened);
  if (server) {
    renderDetail(body, server, {
      groups: listed.groups,
      onBack: () => {
        if (history.state?.server) history.back();
        else {
          opened = null;
          render();
        }
      },
      onGone: () => {
        history.replaceState({ tab: "servers", project: null }, "");
        opened = null;
        loadServers();
      },
    });
    return;
  }
  opened = null;
  renderList(body, listed, { onOpen: open, onRegistered: (id) => loadServers().then(() => open(id)) });
}

export async function loadServers() {
  if (!listed.servers.length) body.replaceChildren(span("list-none", "서버 기록을 읽는 중…"));
  try {
    listed = await invoke("list_servers");
  } catch (err) {
    listed = { servers: [], errors: [String(err)], suggestions: 0, groups: [] };
  }
  for (const message of listed.errors) termWrite("err", message);
  render();
}

/// 서버 목록. 다른 화면이 서버를 고르거나 보여 줄 때 쓴다.
export function knownServers() {
  return listed.servers;
}

listen("servers:updated", loadServers);

document.addEventListener("app:route", (event) => {
  if (event.detail.tab !== "servers") return;
  opened = event.detail.server ?? null;
  render();
});

// 왼쪽 메뉴의 [서버]를 누르면 목록으로 돌아간다.
document.addEventListener("app:nav", (event) => {
  if (event.detail !== "servers" || !opened) return;
  opened = null;
  render();
});
