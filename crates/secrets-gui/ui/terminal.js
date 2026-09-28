// 아래쪽 작업 창. 이 도구가 실행한 모든 명령과 그 출력이 여기로 흐른다.
//
// 자격 증명 도구가 무엇을 하는지 보이지 않으면 믿을 근거가 없다. 그래서 기록은 늘
// 남는다. 다만 창은 평소 한 줄로 접혀 상태만 보인다 — 늘 펼쳐 두면 본문을 먹고,
// 시작할 때 도는 도구 점검으로 찬다. 펼치는 것은 사람이 한다.

import { span } from "./dom.js";

const { listen } = window.__TAURI__.event;

const terminalEl = document.getElementById("terminal");
const termBody = document.getElementById("term-body");
const termStatus = document.getElementById("term-status");
const termClear = document.getElementById("term-clear");

// 실행 중인 job id → 그 실행을 띄운 버튼. 완료 시 되돌리기 위해 들고 있다.
const running = new Map();

// job 이 끝난 뒤 다시 읽어야 하는 것들. 터미널이 무엇을 다시 읽을지 알 필요는 없다.
const afterJob = [];

/** 추적 중이던 job 이 끝나면 부를 것을 등록한다. */
export function onJobFinished(reload) {
  afterJob.push(reload);
}

/** 이 job 이 끝날 때까지 버튼을 붙들어 둔다. */
export function trackJob(job, button) {
  running.set(job, button);
}

export function busy() {
  return running.size > 0;
}


export function termWrite(className, text) {
  const atBottom =
    termBody.scrollTop + termBody.clientHeight >= termBody.scrollHeight - 8;
  termBody.append(span(className, text));
  // 사용자가 위로 올려 읽는 중이면 따라가지 않는다.
  if (atBottom) termBody.scrollTop = termBody.scrollHeight;
}

export function setTermStatus(text, kind = "") {
  termStatus.className = `term-status ${kind}`.trim();
  termStatus.textContent = text;
  // 접혀 있어도 실행 중 · 실패는 한 줄에서 보인다.
  terminalEl.dataset.state = kind;
}

// 프론트엔드에서 난 오류를 조용히 삼키지 않는다. 화면이 부분적으로만 그려지고
// 원인을 알 수 없는 상태가 되는 걸 막는다.
export function reportUiError(what, detail) {
  termWrite("err", `화면 오류 — ${what}: ${detail}`);
  setTermStatus(`화면 오류: ${what}`, "fail");
}

window.addEventListener("error", (e) => {
  reportUiError(e.message, `${e.filename?.split("/").pop() ?? "?"}:${e.lineno}`);
});

window.addEventListener("unhandledrejection", (e) => {
  reportUiError("처리되지 않은 오류가 발생했습니다.", String(e.reason));
});

// 돌고 있는 job id → 명령. 조용히 끝나는 job(성공한 버전 확인 등)도 있어,
// 끝 메시지만 보고는 상태 줄을 되돌릴 수 없다.
const inFlight = new Map();

listen("cli:start", (e) => {
  inFlight.set(e.payload.job, e.payload.command);
  termWrite("cmd", `$ ${e.payload.command}`);
  setTermStatus(`실행 중 — ${e.payload.command}`, "running");
});

const STREAM_CLASS = { err: "err", step: "step" };

listen("cli:line", (e) => {
  const { stream, line } = e.payload;
  termWrite(STREAM_CLASS[stream] ?? "", stream === "step" ? `▸ ${line}` : line);
});

listen("cli:end", (e) => {
  const { job, ok, message } = e.payload;
  const command = inFlight.get(job);
  inFlight.delete(job);
  if (message) {
    termWrite(ok ? "end" : "end fail", message);
    setTermStatus(message, ok ? "ok" : "fail");
  } else if (!ok) {
    setTermStatus(`실패 — ${command ?? "작업"}`, "fail");
  } else if (!inFlight.size && termStatus.classList.contains("running")) {
    setTermStatus("대기 중", "");
  } else if (inFlight.size) {
    setTermStatus(`실행 중 — ${[...inFlight.values()].pop()}`, "running");
  }

  // 설치 job 만 재검사를 유발한다. 버전 검사까지 재검사를 부르면 무한 반복이 된다.
  const button = running.get(job);
  if (!button) return;
  running.delete(job);
  button.restore();

  // 성공이든 실패든 실제 상태를 다시 읽는다. 설치됐다고 가정하지 않는다.
  for (const reload of afterJob) reload();
});

termClear.addEventListener("click", () => {
  termBody.replaceChildren();
  setTermStatus(busy() ? termStatus.textContent : "대기 중", "");
});


const splitter = document.getElementById("splitter");
const toggle = document.getElementById("term-toggle");
const jobsToggle = document.getElementById("open-jobs");

const MIN_TERM = 84;
// 본문이 이만큼은 남아야 한다. 작게 잡으면 창이 줄었을 때 작업 창이 본문을
// 통째로 밀어내고, 남은 칸이 너무 작아 스크롤해도 읽을 게 없어진다.
const MIN_MAIN = 280;
const STORED = "terminalHeight";
const STORED_OPEN = "terminalOpen";
const DEFAULT_HEIGHT = 260;

function storage(action) {
  try {
    return action(window.localStorage);
  } catch {
    return null;
  }
}

function setTerminalHeight(px) {
  const max = Math.max(MIN_TERM, window.innerHeight - MIN_MAIN);
  const height = Math.min(Math.max(px, MIN_TERM), max);
  terminalEl.style.height = `${height}px`;
  return height;
}

function storeTerminalHeight() {
  const height = terminalEl.getBoundingClientRect().height;
  storage((s) => s.setItem(STORED, String(height)));
}

function isOpen() {
  return !terminalEl.classList.contains("collapsed");
}

// 접으면 높이를 비워 머리줄만 남긴다. 펼치면 지난번 높이로 돌아간다.
function setOpen(open) {
  terminalEl.classList.toggle("collapsed", !open);
  splitter.hidden = !open;
  toggle.setAttribute("aria-expanded", String(open));
  jobsToggle.setAttribute("aria-expanded", String(open));
  toggle.querySelector(".term-label").textContent = open ? "작업 로그 접기 ▾" : "작업 로그 펼치기 ▸";
  if (open) {
    const saved = Number(storage((s) => s.getItem(STORED)));
    setTerminalHeight(saved > 0 ? saved : DEFAULT_HEIGHT);
    termBody.scrollTop = termBody.scrollHeight;
  } else {
    terminalEl.style.height = "";
  }
  storage((s) => s.setItem(STORED_OPEN, open ? "1" : "0"));
}

/** 작업 창의 열림 상태를 전환한다. */
export function toggleJobs() {
  setOpen(!isOpen());
}

toggle.addEventListener("click", toggleJobs);

// ⌃` 로 접고 편다. 입력 칸에서 치는 글자는 건드리지 않는다.
window.addEventListener("keydown", (event) => {
  if (event.key !== "`" || !event.ctrlKey || event.metaKey || event.altKey) return;
  event.preventDefault();
  toggleJobs();
});

setOpen(storage((s) => s.getItem(STORED_OPEN)) === "1");

splitter.addEventListener("pointerdown", (event) => {
  event.preventDefault();
  // 포인터를 캡처해 두면 커서가 창 밖으로 나가도 드래그가 이어진다.
  splitter.setPointerCapture(event.pointerId);
  splitter.classList.add("dragging");
  document.body.classList.add("resizing");

  const startY = event.clientY;
  const startH = terminalEl.getBoundingClientRect().height;

  const onMove = (e) => setTerminalHeight(startH + (startY - e.clientY));

  const onUp = () => {
    splitter.classList.remove("dragging");
    document.body.classList.remove("resizing");
    splitter.removeEventListener("pointermove", onMove);
    splitter.removeEventListener("pointerup", onUp);
    splitter.removeEventListener("pointercancel", onUp);
    storeTerminalHeight();
  };

  splitter.addEventListener("pointermove", onMove);
  splitter.addEventListener("pointerup", onUp);
  splitter.addEventListener("pointercancel", onUp);
});

// 더블클릭으로 기본 높이 복귀.
splitter.addEventListener("dblclick", () => {
  setTerminalHeight(DEFAULT_HEIGHT);
  storeTerminalHeight();
});

// 키보드로도 조절되게. 스플리터에 포커스를 두고 위아래 화살표.
splitter.addEventListener("keydown", (event) => {
  const step = event.shiftKey ? 48 : 16;
  const current = terminalEl.getBoundingClientRect().height;
  if (event.key === "ArrowUp") {
    setTerminalHeight(current + step);
  } else if (event.key === "ArrowDown") {
    setTerminalHeight(current - step);
  } else {
    return;
  }
  event.preventDefault();
  storeTerminalHeight();
});

// 창이 작아지면 작업 창이 본문을 다 먹지 않도록 다시 조인다.
window.addEventListener("resize", () => {
  if (isOpen()) setTerminalHeight(terminalEl.getBoundingClientRect().height);
});
