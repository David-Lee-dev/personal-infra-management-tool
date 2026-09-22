// 아래쪽 터미널 패널. 이 도구가 실행한 모든 명령과 그 출력이 여기로 흐른다.
//
// 자격 증명 도구가 무엇을 하는지 보이지 않으면 믿을 근거가 없다. 그래서 패널은
// 늘 떠 있다.

import { span } from "./dom.js";

const { listen } = window.__TAURI__.event;

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
}

// 프론트엔드에서 난 오류를 조용히 삼키지 않는다. 화면이 부분적으로만 그려지고
// 원인을 알 수 없는 상태가 되는 걸 막는다.
export function reportUiError(what, detail) {
  termWrite("err", `UI 오류 — ${what}: ${detail}`);
  setTermStatus(`UI 오류: ${what}`, "fail");
}

window.addEventListener("error", (e) => {
  reportUiError(e.message, `${e.filename?.split("/").pop() ?? "?"}:${e.lineno}`);
});

window.addEventListener("unhandledrejection", (e) => {
  reportUiError("처리되지 않은 오류", String(e.reason));
});

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
  for (const reload of afterJob) reload();
});

termClear.addEventListener("click", () => {
  termBody.replaceChildren();
  setTermStatus(busy() ? termStatus.textContent : "대기 중", "");
});


const terminal = document.getElementById("terminal");
const splitter = document.getElementById("splitter");

const MIN_TERM = 84;
// 본문이 이만큼은 남아야 한다. 작게 잡으면 창이 줄었을 때 터미널이 본문을
// 통째로 밀어내고, 남은 칸이 너무 작아 스크롤해도 읽을 게 없어진다.
const MIN_MAIN = 280;
const STORED = "terminalHeight";

function setTerminalHeight(px) {
  const max = Math.max(MIN_TERM, window.innerHeight - MIN_MAIN);
  const height = Math.min(Math.max(px, MIN_TERM), max);
  terminal.style.height = `${height}px`;
  return height;
}

function storeTerminalHeight() {
  localStorage.setItem(STORED, String(terminal.getBoundingClientRect().height));
}

// 지난 실행에서 쓰던 높이를 되살린다.
const savedHeight = Number(localStorage.getItem(STORED));
setTerminalHeight(savedHeight > 0 ? savedHeight : 216);

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
    storeTerminalHeight();
  };

  splitter.addEventListener("pointermove", onMove);
  splitter.addEventListener("pointerup", onUp);
  splitter.addEventListener("pointercancel", onUp);
});

// 더블클릭으로 기본 높이 복귀.
splitter.addEventListener("dblclick", () => {
  setTerminalHeight(216);
  storeTerminalHeight();
});

// 키보드로도 조절되게. 스플리터에 포커스를 두고 위아래 화살표.
splitter.addEventListener("keydown", (event) => {
  const step = event.shiftKey ? 48 : 16;
  const current = terminal.getBoundingClientRect().height;
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

// 창이 작아지면 터미널이 본문을 다 먹지 않도록 다시 조인다.
window.addEventListener("resize", () => {
  setTerminalHeight(terminal.getBoundingClientRect().height);
});
