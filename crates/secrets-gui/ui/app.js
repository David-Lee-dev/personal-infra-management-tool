const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const rows = document.getElementById("rows");
const summary = document.getElementById("summary");
const refresh = document.getElementById("refresh");
const consolePanel = document.getElementById("console");
const consoleTitle = document.getElementById("console-title");
const consoleBody = document.getElementById("console-body");
const consoleClose = document.getElementById("console-close");

// 설치가 진행 중인 툴 id. 같은 툴을 두 번 누르는 걸 막는다.
let installing = null;

// 백엔드에서 온 문자열을 그대로 innerHTML 에 넣지 않는다.
function escape(text) {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

function cell(node) {
  const td = document.createElement("td");
  td.append(node);
  return td;
}

function span(className, text) {
  const el = document.createElement("span");
  el.className = className;
  el.textContent = text;
  return el;
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
    tr.append(cell(statusCell(tool)));
    tr.append(cell(span("when", tool.requirement)));
    rows.append(tr);
  }
  return tools;
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

async function load() {
  refresh.disabled = true;
  // 결과가 같아도 검사가 돌았다는 게 보여야 한다.
  setSummary("검사 중…");
  try {
    const tools = render(await invoke("list_tools"));
    const found = tools.filter((t) => t.path).length;
    setSummary(`${tools.length}개 중 ${found}개 설치됨 · ${now()} 확인`);
  } catch (err) {
    setSummary(`검사 실패: ${err}`, "fail");
  } finally {
    refresh.disabled = false;
  }
}

function openConsole(title) {
  consoleTitle.textContent = title;
  consoleBody.textContent = "";
  consolePanel.hidden = false;
}

function appendLog(line) {
  const atBottom =
    consoleBody.scrollTop + consoleBody.clientHeight >= consoleBody.scrollHeight - 8;
  consoleBody.textContent += `${line}\n`;
  // 사용자가 위로 올려 읽는 중이면 따라가지 않는다.
  if (atBottom) consoleBody.scrollTop = consoleBody.scrollHeight;
}

async function startInstall(tool, button) {
  if (installing) return;
  installing = tool.id;

  button.disabled = true;
  button.className = "install busy";
  button.textContent = "설치 중…";
  openConsole(`${tool.id} — ${tool.install}`);
  appendLog(`$ ${tool.install}`);

  try {
    await invoke("install_tool", { id: tool.id });
  } catch (err) {
    installing = null;
    appendLog(String(err));
    setSummary(`${tool.id} 설치 실패: ${err}`, "fail");
    button.disabled = false;
    button.className = "install";
    button.textContent = `설치  ${tool.install}`;
  }
}

listen("install:log", (event) => {
  if (event.payload.id === installing) appendLog(event.payload.line);
});

listen("install:done", async (event) => {
  const { id, ok, message } = event.payload;
  if (id !== installing) return;
  installing = null;
  appendLog(message);
  setSummary(`${id}: ${message}`, ok ? "ok" : "fail");
  // 성공이든 실패든 실제 상태를 다시 읽는다. 설치됐다고 가정하지 않는다.
  await load();
});

consoleClose.addEventListener("click", () => {
  consolePanel.hidden = true;
});

refresh.addEventListener("click", load);
load();
