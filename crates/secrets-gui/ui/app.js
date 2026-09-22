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

listen("cli:end", (e) => {
  const { job, ok, message } = e.payload;
  if (message) {
    termWrite(ok ? "end" : "end fail", message);
    setTermStatus(message, ok ? "ok" : "fail");
  }

  // 설치 job 만 재검사를 유발한다. 버전 검사까지 재검사를 부르면 무한 반복이 된다.
  const button = running.get(job);
  if (!button) return;
  running.delete(job);
  button.restore();

  // 성공이든 실패든 실제 상태를 다시 읽는다. 설치됐다고 가정하지 않는다.
  load();
});

termClear.addEventListener("click", () => {
  termBody.replaceChildren();
  setTermStatus(running.size ? termStatus.textContent : "대기 중", "");
});

/* ── 터미널 높이 조절 ───────────────────────────────── */

const terminal = document.getElementById("terminal");
const splitter = document.getElementById("splitter");

const MIN_H = 84;
const MAX_MARGIN = 140; // 본문이 이만큼은 남아야 한다
const STORED = "terminalHeight";

function setTerminalHeight(px) {
  const max = Math.max(MIN_H, window.innerHeight - MAX_MARGIN);
  const height = Math.min(Math.max(px, MIN_H), max);
  terminal.style.height = `${height}px`;
  return height;
}

// 지난 실행에서 쓰던 높이를 되살린다.
const saved = Number(localStorage.getItem(STORED));
if (saved) setTerminalHeight(saved);

splitter.addEventListener("pointerdown", (event) => {
  event.preventDefault();
  // 포인터를 캡처해 두면 커서가 창 밖으로 나가도 드래그가 이어진다.
  splitter.setPointerCapture(event.pointerId);
  splitter.classList.add("dragging");
  document.body.classList.add("resizing");

  const startY = event.clientY;
  const startH = terminal.getBoundingClientRect().height;

  const onMove = (e) => setTerminalHeight(startH + (startY - e.clientY));

  const onUp = () => {
    splitter.classList.remove("dragging");
    document.body.classList.remove("resizing");
    splitter.removeEventListener("pointermove", onMove);
    splitter.removeEventListener("pointerup", onUp);
    splitter.removeEventListener("pointercancel", onUp);
    localStorage.setItem(STORED, terminal.getBoundingClientRect().height);
  };

  splitter.addEventListener("pointermove", onMove);
  splitter.addEventListener("pointerup", onUp);
  splitter.addEventListener("pointercancel", onUp);
});

// 더블클릭으로 기본 높이 복귀.
splitter.addEventListener("dblclick", () => {
  localStorage.setItem(STORED, setTerminalHeight(216));
});

// 키보드로도 조절되게. 스플리터에 포커스를 두고 위아래 화살표.
splitter.addEventListener("keydown", (event) => {
  const step = event.shiftKey ? 48 : 16;
  const current = terminal.getBoundingClientRect().height;
  if (event.key === "ArrowUp") {
    localStorage.setItem(STORED, setTerminalHeight(current + step));
  } else if (event.key === "ArrowDown") {
    localStorage.setItem(STORED, setTerminalHeight(current - step));
  } else {
    return;
  }
  event.preventDefault();
});

// 창이 작아지면 터미널이 본문을 다 먹지 않도록 다시 조인다.
window.addEventListener("resize", () => {
  setTerminalHeight(terminal.getBoundingClientRect().height);
});

/* ── 툴 목록 ────────────────────────────────────────── */

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
    tr.append(cell(statusCell(tool)));
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

// 검사는 버전 명령을 실제로 돌리므로 비동기다. 결과는 tools:updated 로 돌아온다.
function load() {
  refresh.disabled = true;
  setSummary("검사 중…");
  invoke("inspect").catch((err) => {
    setSummary(`검사 실패: ${err}`, "fail");
    refresh.disabled = false;
  });
}

listen("tools:updated", (e) => {
  const { tools, total, found, blocking } = e.payload;
  render(tools);
  refresh.disabled = false;

  if (blocking.length) {
    setSummary(
      `${total}개 중 ${found}개 설치됨 · 필수 툴 미충족: ${blocking.join(", ")} · ${now()} 확인`,
      "fail",
    );
  } else {
    setSummary(`${total}개 중 ${found}개 설치됨 · ${now()} 확인`, "ok");
  }
});

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
