const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const rows = document.getElementById("rows");
const summary = document.getElementById("summary");
const refresh = document.getElementById("refresh");
const termBody = document.getElementById("term-body");
const termStatus = document.getElementById("term-status");
const termClear = document.getElementById("term-clear");

// 실행 중인 job id → 그 실행을 띄운 버튼. 완료 시 되돌리기 위해 들고 있다.
const running = new Map();

function span(className, text) {
  const el = document.createElement("span");
  el.className = className;
  el.textContent = text;
  return el;
}

function cell(node) {
  const td = document.createElement("td");
  td.append(node);
  return td;
}

/* ── 터미널 ─────────────────────────────────────────── */

function termWrite(className, text) {
  const atBottom =
    termBody.scrollTop + termBody.clientHeight >= termBody.scrollHeight - 8;
  termBody.append(span(className, text));
  // 사용자가 위로 올려 읽는 중이면 따라가지 않는다.
  if (atBottom) termBody.scrollTop = termBody.scrollHeight;
}

function setTermStatus(text, kind = "") {
  termStatus.className = `term-status ${kind}`.trim();
  termStatus.textContent = text;
}

listen("cli:start", (e) => {
  termWrite("cmd", `$ ${e.payload.command}`);
  setTermStatus(`실행 중 — ${e.payload.command}`, "running");
});

listen("cli:line", (e) => {
  termWrite(e.payload.stream === "err" ? "err" : "", e.payload.line);
});

listen("cli:end", async (e) => {
  const { job, ok, message } = e.payload;
  termWrite(ok ? "end" : "end fail", message);
  setTermStatus(message, ok ? "ok" : "fail");

  const button = running.get(job);
  running.delete(job);
  if (button) button.restore();

  // 성공이든 실패든 실제 상태를 다시 읽는다. 설치됐다고 가정하지 않는다.
  await load();
});

termClear.addEventListener("click", () => {
  termBody.replaceChildren();
  setTermStatus(running.size ? termStatus.textContent : "대기 중", "");
});

/* ── 툴 목록 ────────────────────────────────────────── */

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

async function startInstall(tool, button) {
  button.disabled = true;
  button.className = "install busy";
  button.textContent = "설치 중…";

  try {
    const job = await invoke("install_tool", { id: tool.id });
    running.set(job, button);
  } catch (err) {
    termWrite("err", String(err));
    setTermStatus(String(err), "fail");
    button.restore();
  }
}

refresh.addEventListener("click", load);
load();
