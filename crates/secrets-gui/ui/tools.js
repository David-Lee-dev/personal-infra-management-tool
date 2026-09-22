// 환경 구성 탭 — 이 머신에 필요한 CLI 가 갖춰져 있는가.

import { cell, span } from "./dom.js";
import { setTermStatus, termWrite, trackJob } from "./terminal.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;


const rows = document.getElementById("rows");
const summary = document.getElementById("summary");
const refresh = document.getElementById("refresh");

function versionCell(tool) {
  if (!tool.path) return span("version unknown", "-");

  if (!tool.version) {
    // 파싱에 실패했을 때는 원문을 보여준다. 숨기면 원인을 알 수 없다.
    const el = span("version unknown", "확인 불가");
    el.title = tool.version_raw || "";
    return el;
  }

  if (tool.meets_minimum) return span("version", tool.version);

  const wrap = document.createDocumentFragment();
  wrap.append(span("version stale", tool.version));
  wrap.append(span("reason", `${tool.minimum} 이상 필요 — ${tool.minimum_reason}`));
  return wrap;
}

// 계정 격리는 이 앱의 기본 동작이다. 성립할 때는 아무것도 표시하지 않고,
// 깨졌을 때만 왜 이 툴을 쓸 수 없는지 알린다.
function isolationProblem(tool) {
  if (tool.isolation === "leaked") {
    return span(
      "problem",
      `계정 격리 불가 — ${tool.isolation_env} 를 무시합니다. 계정을 여러 개 붙이면 엉뚱한 계정으로 실행될 수 있어 사용할 수 없습니다.`,
    );
  }
  if (tool.isolation === "inconclusive") {
    return span("note", `계정 격리를 확인하지 못했습니다 — ${tool.isolation_evidence}`);
  }
  return null;
}

function statusCell(tool) {
  if (tool.path) return span("path found", tool.path);

  const wrap = document.createDocumentFragment();
  wrap.append(span("missing", "설치되지 않음"));

  if (tool.installable) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "install";
    button.textContent = `설치  ${tool.install}`;
    button.restore = () => {
      button.disabled = false;
      button.className = "install";
      button.textContent = `설치  ${tool.install}`;
    };
    button.addEventListener("click", () => startInstall(tool, button));
    wrap.append(document.createElement("br"), button);
  } else {
    wrap.append(span("hint", tool.install));
  }
  return wrap;
}

function render(tools) {
  rows.replaceChildren();
  for (const tool of tools) {
    const tr = document.createElement("tr");
    tr.append(cell(span("name", tool.id)));
    tr.append(cell(versionCell(tool)));

    const status = document.createDocumentFragment();
    status.append(statusCell(tool));
    const problem = isolationProblem(tool);
    if (problem) status.append(problem);
    tr.append(cell(status));

    tr.append(cell(span("when", tool.requirement)));
    rows.append(tr);
  }
}

function setSummary(text, kind = "") {
  summary.className = `summary ${kind}`.trim();
  summary.textContent = text;
}

function now() {
  return new Date().toLocaleTimeString("ko-KR", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

// 검사는 버전·격리 명령을 실제로 돌리므로 비동기다. 결과는 tools:updated 로 돌아온다.
export function loadTools() {
  refresh.disabled = true;
  setSummary("검사 중…");
  invoke("inspect").catch((err) => {
    setSummary(`검사 실패: ${err}`, "fail");
    refresh.disabled = false;
  });
}

listen("tools:updated", (e) => {
  const { tools, total, found, blocking, isolated, isolationChecked } = e.payload;
  render(tools);
  refresh.disabled = false;

  const base = `${total}개 중 ${found}개 설치됨 · 계정 격리 ${isolated}/${isolationChecked} 확인`;
  if (blocking.length) {
    setSummary(`${base} · 사용 불가: ${blocking.join(", ")} · ${now()}`, "fail");
  } else {
    setSummary(`${base} · ${now()}`, "ok");
  }
});

async function startInstall(tool, button) {
  button.disabled = true;
  button.className = "install busy";
  button.textContent = "설치 중…";

  try {
    const job = await invoke("install_tool", { id: tool.id });
    trackJob(job, button);
  } catch (err) {
    termWrite("err", String(err));
    setTermStatus(String(err), "fail");
    button.restore();
  }
}
