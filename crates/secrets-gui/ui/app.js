const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

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

// 프론트엔드에서 난 오류를 조용히 삼키지 않는다. 화면이 부분적으로만 그려지고
// 원인을 알 수 없는 상태가 되는 걸 막는다.
function reportUiError(what, detail) {
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
  load();
});

termClear.addEventListener("click", () => {
  termBody.replaceChildren();
  setTermStatus(running.size ? termStatus.textContent : "대기 중", "");
});

/* ── 환경 구성 — CLI 점검 ───────────────────────────── */

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
function load() {
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
    running.set(job, button);
  } catch (err) {
    termWrite("err", String(err));
    setTermStatus(String(err), "fail");
    button.restore();
  }
}

/* ── 터미널 높이 조절 ───────────────────────────────── */

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

/* ── 탭 ─────────────────────────────────────────────── */

const tabBar = document.getElementById("tabs");
const panels = {
  env: document.getElementById("tab-env"),
  accounts: document.getElementById("tab-accounts"),
};

// 탭을 처음 열 때만 데이터를 읽는다.
const loaded = new Set();

function showTab(name) {
  for (const button of tabBar.querySelectorAll("button")) {
    button.setAttribute("aria-selected", String(button.dataset.tab === name));
  }
  for (const [key, panel] of Object.entries(panels)) {
    panel.hidden = key !== name;
  }
  if (!loaded.has(name)) {
    loaded.add(name);
    if (name === "accounts") loadAccounts();
  }
}

tabBar.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-tab]");
  if (button) showTab(button.dataset.tab);
});

/* ── 계정 관리 — 좌측 목록 + 우측 상세 ─────────────────── */

const alertBar = document.getElementById("alerts");
const rail = document.getElementById("rail");
const detail = document.getElementById("detail");
const formTemplate = document.getElementById("tpl-form");

const PROVIDERS = [
  { id: "github", label: "GitHub" },
  { id: "aws", label: "AWS" },
  { id: "gcloud", label: "Google Cloud" },
  { id: "firebase", label: "Firebase" },
];

function providerLabelOf(id) {
  return PROVIDERS.find((p) => p.id === id)?.label ?? id;
}

// 지금 화면에 띄운 것. {kind: "account", ref} | {kind: "new", provider} | null
let selection = null;
let accounts = [];
const EXPIRY_LABEL = {
  expired: (d) => (d === 0 ? "오늘 만료" : `${d}일 전 만료됨`),
  soon: (d) => (d === 0 ? "오늘 만료" : `${d}일 남음`),
  ok: () => "유효",
  never: () => "기한 없음",
  unset: () => "확인 안 됨",
};

function expiryText(acc) {
  return (EXPIRY_LABEL[acc.expiry] ?? (() => acc.expiry))(acc.expiry_days ?? 0);
}

function refOf(acc) {
  return `${acc.provider}/${acc.slug}`;
}

/* ── 레일 ───────────────────────────────────────────── */

function railItem(acc) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "rail-item";
  button.setAttribute(
    "aria-current",
    String(
      (selection?.kind === "account" || selection?.kind === "reissue") &&
        selection.ref === refOf(acc),
    ),
  );

  // 문제를 레일에서 바로 본다. 만료가 검증 실패보다 급하다.
  const expiring = acc.expiry === "soon" || acc.expiry === "expired";
  const state = expiring
    ? " warn"
    : acc.verified_ok === true
      ? ""
      : acc.verified_ok === false
        ? " warn"
        : " unknown";
  button.append(span(`dot${state}`, ""));
  button.append(span("slug", acc.slug));
  // 지금 전역으로 쓰이는 계정. 터미널에서 치는 명령이 이 계정으로 나간다.
  if (acc.is_active) button.append(span("rail-active", "사용 중"));
  button.title = acc.display || acc.slug;

  button.addEventListener("click", () => select({ kind: "account", ref: refOf(acc) }));
  return button;
}

function renderRail() {
  rail.replaceChildren();

  for (const provider of PROVIDERS) {
    const group = document.createElement("div");
    group.className = "rail-group";
    group.append(span("cap", provider.label));

    const add = document.createElement("button");
    add.type = "button";
    add.className = "rail-add";
    add.textContent = "＋";
    add.title = `${provider.label} 계정 추가`;
    add.addEventListener("click", () => select({ kind: "new", provider: provider.id }));
    group.append(add);
    rail.append(group);

    const mine = accounts.filter((a) => a.provider === provider.id);
    if (!mine.length) {
      rail.append(span("rail-none", "없음"));
      continue;
    }
    for (const acc of mine) rail.append(railItem(acc));
  }

  // 항목이 적어도 레일이 위로 뭉치지 않게 남는 공간을 채운다.
  const filler = document.createElement("div");
  filler.className = "rail-filler";
  rail.append(filler);
}

/* ── 상세 ───────────────────────────────────────────── */

function pane(title, ...children) {
  const box = document.createElement("section");
  box.className = "pane";

  const head = document.createElement("div");
  head.className = "pane-head";
  head.append(span("cap", title));
  box.append(head);

  box.append(...children);
  return box;
}

function facts(pairs) {
  const list = document.createElement("dl");
  list.className = "facts";
  for (const [label, value, mono] of pairs) {
    const dt = document.createElement("dt");
    dt.textContent = label;
    const dd = document.createElement("dd");
    if (mono) dd.className = "mono";
    dd.textContent = value;
    list.append(dt, dd);
  }
  return list;
}

function button(label, { primary = false, onClick } = {}) {
  const el = document.createElement("button");
  el.type = "button";
  el.textContent = label;
  if (primary) el.className = "primary";
  if (onClick) el.addEventListener("click", onClick);
  return el;
}

function placeholder(title, body) {
  const box = document.createElement("div");
  box.className = "placeholder";
  const strong = document.createElement("strong");
  strong.textContent = title;
  const p = document.createElement("p");
  p.textContent = body;
  box.append(strong, p);
  return box;
}

// 이 계정에 할 수 있는 일. 머리말 오른쪽에 모아 둔다.
function headActions(acc) {
  const box = document.createElement("div");
  box.className = "head-actions";

  box.append(
    button("다시 검증", {
      onClick: () => invoke("verify_account", { provider: acc.provider, slug: acc.slug }),
    }),
  );

  // 기한이 없는 자격도 회전할 수 있어야 하므로 늘 열어 둔다.
  box.append(button("재발급", { onClick: () => openReissue(acc) }));

  if (acc.is_active) {
    box.append(
      button("해제", {
        onClick: () => invoke("deactivate_provider", { provider: acc.provider }),
      }),
    );
  } else if (acc.global_path) {
    box.append(
      button("할당", {
        primary: true,
        onClick: () =>
          invoke("activate_account", { provider: acc.provider, slug: acc.slug }).catch((err) =>
            termWrite("err", String(err)),
          ),
      }),
    );
  }
  return box;
}

// AWS 계정 상태. root 관련은 우리가 다루지 않지만 상태는 알려 준다.
function awsPane(acc) {
  if (acc.root_keys_present === null && acc.root_mfa === null) return null;

  const rows = [];
  if (acc.root_keys_present !== null) {
    rows.push(["root 키", acc.root_keys_present ? "있음" : "없음"]);
  }
  if (acc.root_mfa !== null) {
    rows.push(["root MFA", acc.root_mfa ? "켜짐" : "꺼짐"]);
  }

  const box = pane("계정 상태", facts(rows));

  // root 자격은 이 도구가 보관하지 않는다. 문제가 있을 때만 말한다.
  const problems = [];
  if (acc.root_keys_present) {
    problems.push("root 액세스 키가 있습니다. AWS 는 삭제를 권고합니다.");
  }
  if (acc.root_mfa === false) {
    problems.push("root MFA 가 꺼져 있습니다.");
  }
  for (const text of problems) {
    const p = document.createElement("p");
    p.className = "problem";
    p.textContent = text;
    box.append(p);
  }
  return box;
}

// 전역 적용 상태. 버튼은 머리말로 올라갔고 여기엔 사실만 남는다.
function activePane(acc) {
  const rows = [["설정 홈", acc.cli_home, true]];
  if (acc.global_path) rows.push(["전역 설정", acc.global_path, true]);
  if (acc.git_email) rows.push(["커밋 이메일", acc.git_email, true]);

  const box = pane("격리", facts(rows));

  // 주의가 필요할 때만 말한다. 평소 동작은 설명하지 않는다.
  if (acc.caution) {
    const caution = document.createElement("p");
    caution.className = "problem";
    caution.textContent = acc.caution;
    box.append(caution);
  }
  return box;
}

function renderAccount(acc) {
  const head = document.createElement("div");
  head.className = "detail-head";

  const titleWrap = document.createElement("div");
  titleWrap.className = "detail-title-wrap";
  titleWrap.append(span("cap", providerLabelOf(acc.provider)));

  const line = document.createElement("div");
  line.className = "detail-title-line";
  const h2 = document.createElement("h2");
  h2.textContent = acc.slug;
  line.append(h2);
  // 지금 이 계정으로 gh 명령이 나가는지. 제목 옆이 제일 먼저 눈에 든다.
  if (acc.is_active) line.append(span("badge-active", "사용 중"));
  titleWrap.append(line);

  if (acc.display) titleWrap.append(span("detail-sub", acc.display));
  head.append(titleWrap);
  head.append(headActions(acc));

  const body = document.createElement("div");
  body.className = "detail-body";

  const verified =
    acc.verified_ok === true
      ? `확인됨 · ${acc.verified_at ?? ""}`
      : acc.verified_ok === false
        ? acc.verified_detail || "확인 실패"
        : "아직 확인하지 않음";

  body.append(
    pane(
      "신원",
      facts(
        [
          ["로그인", acc.identity_name || "미확인", true],
          ["방식", acc.identity_kind || "—"],
          acc.aws_account_id && ["AWS 계정", acc.aws_account_id, true],
          ["검증", verified],
        ].filter(Boolean),
      ),
    ),
  );

  // 만료는 검증보다 위에 둔다. 기한이 지나면 나머지가 다 의미를 잃는다.
  const expiryPane = pane(
    "자격 기한",
    facts([
      ["만료일", acc.expiry === "never" ? "없음" : acc.expires || "확인 안 됨"],
      ["상태", expiryText(acc)],
    ]),
  );
  if (acc.expiry === "never") {
    const warn = document.createElement("p");
    warn.className = "pane-note";
    // 무기한 자격은 유출돼도 스스로 만료되지 않는다. 알림은 안 띄우되 짚어는 둔다.
    warn.textContent =
      "기한이 없는 자격입니다. 유출되어도 스스로 만료되지 않으니 주기적으로 직접 회전하세요.";
    expiryPane.append(warn);
  }
  if (acc.expiry === "soon" || acc.expiry === "expired") {
    const hint = document.createElement("p");
    hint.className = "pane-note";
    hint.textContent = acc.renewal_hint;
    expiryPane.append(hint);
  }
  body.append(expiryPane);

  const aws = awsPane(acc);
  if (aws) body.append(aws);

  body.append(activePane(acc));
  detail.replaceChildren(head, body);
}

// 자격 교체. 계정은 그대로 두고 값만 갈아 끼운다.
function openReissue(acc) {
  selection = { kind: "reissue", ref: refOf(acc) };
  renderRail();
  renderDetail();
}

function renderReissue(acc) {
  const form = formTemplate.content.cloneNode(true).querySelector("form");
  detail.replaceChildren(form);
  bindReissue(form, acc);
}

function renderForm(providerId) {
  const form = formTemplate.content.cloneNode(true).querySelector("form");
  detail.replaceChildren(form);
  bindForm(form, providerId);
}

function renderEmpty() {
  detail.replaceChildren(
    accounts.length
      ? placeholder("계정을 고르세요", "왼쪽에서 계정을 누르면 신원과 격리 상태를 볼 수 있습니다.")
      : placeholder(
          "등록된 계정이 없습니다",
          "왼쪽 provider 옆 ＋ 를 눌러 계정을 추가하세요. 계정마다 CLI 설정 홈이 따로 만들어지고, 로그인은 그 안에서만 이뤄집니다. 기존 로그인은 건드리지 않습니다.",
        ),
  );
}

function renderDetail() {
  if (selection?.kind === "new") return renderForm(selection.provider);

  if (selection?.kind === "reissue") {
    const acc = accounts.find((a) => refOf(a) === selection.ref);
    if (acc) return renderReissue(acc);
  }

  if (selection?.kind === "account") {
    const acc = accounts.find((a) => refOf(a) === selection.ref);
    if (acc) return renderAccount(acc);
    // 방금 만든 계정은 목록에 아직 없을 수 있다. 선택을 지우지 않는다 —
    // 지우면 목록이 도착해도 상세가 열리지 않는다. 정리는 loadAccounts 가 한다.
  }
  renderEmpty();
}

function select(next) {
  selection = next;
  renderRail();
  renderDetail();
}

function renderAlerts(messages) {
  alertBar.replaceChildren();
  alertBar.hidden = !messages.length;
  for (const message of messages) {
    alertBar.append(span("alert", message));
  }
}

async function loadAccounts() {
  try {
    const result = await invoke("list_accounts");
    accounts = result.accounts;
    renderAlerts(result.alerts);

    // 읽지 못한 항목을 조용히 숨기면 계정이 사라진 것처럼 보인다.
    for (const message of result.errors) termWrite("err", message);
  } catch (err) {
    accounts = [];
    termWrite("err", `계정 목록을 읽지 못했습니다: ${err}`);
  }

  // 선택한 계정이 사라졌으면 선택을 비운다. 폼은 열어 둔 채로 둔다.
  if (selection?.kind === "account" && !accounts.some((a) => refOf(a) === selection.ref)) {
    selection = null;
  }
  renderRail();
  renderDetail();
}

listen("accounts:updated", loadAccounts);

/* ── 자격 재발급 ────────────────────────────────────── */

function bindReissue(form, acc) {
  const fTitle = form.querySelector("#f-title");
  const fGuidance = form.querySelector("#f-guidance");
  const fFields = form.querySelector("#f-fields");
  const fBrowser = form.querySelector("#f-browser");
  const fProbe = form.querySelector("#f-probe");
  const fIdentity = form.querySelector("#f-identity");
  const fDisplay = form.querySelector("#f-display");
  const fSubmit = form.querySelector("#f-submit");
  const fCancel = form.querySelector("#f-cancel");
  const fError = form.querySelector("#f-error");

  form.querySelector(".cap").textContent = "자격 교체";
  fTitle.textContent = `${acc.slug} 재발급`;

  // 설명은 계정에 딸린 것이지 자격에 딸린 것이 아니다. 바꿀 일이 없다.
  fDisplay.closest(".field").hidden = true;

  let spec = null;
  let probed = null;

  function showError(message) {
    fError.textContent = message;
    fError.hidden = !message;
  }

  function collectValues() {
    const values = {};
    for (const input of fFields.querySelectorAll("input")) {
      values[input.dataset.key] = input.value;
    }
    return values;
  }

  function invalidate() {
    probed = null;
    fIdentity.hidden = true;
    fSubmit.disabled = true;
  }

  async function load() {
    showError("");
    fFields.replaceChildren();
    invalidate();

    try {
      spec = await invoke("provider_form", { provider: acc.provider });
    } catch (err) {
      showError(String(err));
      return;
    }

    // 기한을 늘리는 방법이 없다는 걸 여기서 한 번 더 말한다.
    fGuidance.textContent = `${acc.renewal_hint} 같은 계정(${acc.identity_name})의 자격이어야 합니다.`;

    for (const field of spec.fields) {
      const wrap = document.createElement("div");
      wrap.className = "field";

      const label = document.createElement("label");
      label.htmlFor = `v-${field.key}`;
      label.textContent = field.label + (field.required ? "" : " (선택)");
      wrap.append(label);

      const input = document.createElement("input");
      input.id = `v-${field.key}`;
      input.type = field.secret ? "password" : "text";
      input.dataset.key = field.key;
      input.autocomplete = "off";
      input.spellcheck = false;
      input.addEventListener("input", invalidate);
      wrap.append(input);

      if (field.help) wrap.append(span("field-help", field.help));
      fFields.append(wrap);
    }

    fBrowser.hidden = !spec.browser_url;
    if (spec.browser_url) fBrowser.textContent = spec.browser_label;
    fProbe.disabled = spec.fields.length === 0;
    fFields.querySelector("input")?.focus();
  }

  function showIdentity(result) {
    fIdentity.replaceChildren();
    fIdentity.hidden = false;
    fIdentity.append(span("identity-name", result.name));

    const row = (label, value, cls = "") => {
      const el = document.createElement("div");
      el.className = "identity-row";
      el.append(span("identity-label", label));
      el.append(span(`identity-value ${cls}`.trim(), value));
      return el;
    };

    fIdentity.append(
      row(
        "새 만료",
        result.expires === "never" ? "기한 없음" : (result.expires ?? "확인 못 함"),
      ),
    );
    if (result.scopes.length) {
      fIdentity.append(row("scope", result.scopes.join(", "), "mono wrap"));
    }
  }

  async function probe() {
    showError("");
    fProbe.disabled = true;
    fProbe.textContent = "확인 중…";

    try {
      const result = await invoke("probe_credentials", {
        provider: acc.provider,
        values: collectValues(),
      });

      // 다른 계정 자격이면 여기서 막는다. 붙이고 나면 되돌리기 어렵다.
      if (result.name !== acc.identity_name) {
        invalidate();
        showError(
          `다른 계정의 자격입니다. 이 계정은 ${acc.identity_name} 인데 넣은 자격은 ${result.name} 입니다.`,
        );
        return;
      }

      probed = result;
      showIdentity(result);
      fSubmit.disabled = false;
    } catch (err) {
      invalidate();
      showError(String(err));
    } finally {
      fProbe.disabled = false;
      fProbe.textContent = "자격 확인";
    }
  }

  fProbe.addEventListener("click", probe);
  fCancel.addEventListener("click", () => select({ kind: "account", ref: refOf(acc) }));

  fBrowser.addEventListener("click", () => {
    if (spec?.browser_url) {
      invoke("open_url", { url: spec.browser_url }).catch((err) => showError(String(err)));
    }
  });

  fSubmit.textContent = "교체";
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!probed) return;
    showError("");
    fSubmit.disabled = true;

    try {
      await invoke("replace_credential", {
        provider: acc.provider,
        slug: acc.slug,
        values: collectValues(),
      });
      select({ kind: "account", ref: refOf(acc) });
    } catch (err) {
      showError(String(err));
      fSubmit.disabled = false;
    }
  });

  load();
}

/* ── 계정 추가 폼 ───────────────────────────────────── */

function bindForm(form, providerId) {
  const fTitle = form.querySelector("#f-title");
  const fGuidance = form.querySelector("#f-guidance");
  const fFields = form.querySelector("#f-fields");
  const fBrowser = form.querySelector("#f-browser");
  const fProbe = form.querySelector("#f-probe");
  const fIdentity = form.querySelector("#f-identity");
  const fDisplay = form.querySelector("#f-display");
  const fSubmit = form.querySelector("#f-submit");
  const fCancel = form.querySelector("#f-cancel");
  const fError = form.querySelector("#f-error");

  const providerLabel = providerLabelOf(providerId);
  fTitle.textContent = `${providerLabel} 계정 연결`;

  let spec = null;
  // 확인으로 알아낸 사실. 이름과 만료일은 여기서만 온다.
  let probed = null;

  function showError(message) {
    fError.textContent = message;
    fError.hidden = !message;
  }

  function collectValues() {
    const values = {};
    for (const input of fFields.querySelectorAll("input")) {
      values[input.dataset.key] = input.value;
    }
    return values;
  }

  // 자격을 고치면 앞서 확인한 사실은 더 이상 유효하지 않다.
  function invalidate() {
    probed = null;
    fIdentity.hidden = true;
    fSubmit.disabled = true;
  }

  async function loadProviderForm() {
    showError("");
    fFields.replaceChildren();
    invalidate();

    try {
      spec = await invoke("provider_form", { provider: providerId });
    } catch (err) {
      showError(String(err));
      return;
    }

    fGuidance.textContent = spec.guidance;

    if (!spec.tool_ready) {
      showError(`${spec.tool} 가 설치돼 있지 않습니다. 환경 구성 탭에서 먼저 설치하세요.`);
    }

    for (const field of spec.fields) {
      const wrap = document.createElement("div");
      wrap.className = "field";

      const label = document.createElement("label");
      label.htmlFor = `v-${field.key}`;
      label.textContent = field.label + (field.required ? "" : " (선택)");
      wrap.append(label);

      const input = document.createElement("input");
      input.id = `v-${field.key}`;
      input.type = field.secret ? "password" : "text";
      input.dataset.key = field.key;
      // 비밀값이 자동완성에 남지 않게.
      input.autocomplete = "off";
      input.spellcheck = false;
      input.addEventListener("input", invalidate);
      wrap.append(input);

      if (field.help) wrap.append(span("field-help", field.help));
      fFields.append(wrap);
    }

    fBrowser.hidden = !spec.browser_url;
    if (spec.browser_url) fBrowser.textContent = spec.browser_label;

    // 입력할 값이 없는 provider 는 아직 연결 수단이 없다.
    fProbe.disabled = spec.fields.length === 0;
    fFields.querySelector("input")?.focus();
  }

  // root 상태. AWS 가 만들지 말라고 권고하는 것들이라 문제일 때만 눈에 띄게 한다.
  function rootFacts(result) {
    const rows = [];
    if (result.root_keys_present !== null && result.root_keys_present !== undefined) {
      rows.push([
        "root 키",
        result.root_keys_present ? "있음 — 삭제를 권고합니다" : "없음",
        result.root_keys_present ? "warn" : "muted",
      ]);
    }
    if (result.root_mfa !== null && result.root_mfa !== undefined) {
      rows.push([
        "root MFA",
        result.root_mfa ? "켜짐" : "꺼짐 — 켜는 것을 권고합니다",
        result.root_mfa ? "muted" : "warn",
      ]);
    }
    return rows;
  }

  function fact(label, value, className = "") {
    const row = document.createElement("div");
    row.className = "identity-row";
    row.append(span("identity-label", label));
    row.append(span(`identity-value ${className}`.trim(), value));
    return row;
  }

  function showIdentity(result) {
    fIdentity.replaceChildren();
    fIdentity.hidden = false;

    fIdentity.append(span("identity-name", result.name));
    fIdentity.append(fact("계정 이름", result.slug, "mono"));
    if (result.aws_account_id) {
      fIdentity.append(fact("AWS 계정", result.aws_account_id, "mono"));
    }
    for (const row of rootFacts(result)) fIdentity.append(fact(...row));
    fIdentity.append(
      fact(
        "자격 만료",
        result.expires === "never" ? "기한 없음" : (result.expires ?? "확인 못 함"),
        result.expires === "never" ? "muted" : "",
      ),
    );
    if (result.scopes.length) {
      fIdentity.append(fact("scope", result.scopes.join(", "), "mono wrap"));
    }
  }

  async function probe() {
    showError("");
    fProbe.disabled = true;
    fProbe.textContent = spec.browser_login ? "브라우저에서 진행하세요…" : "확인 중…";

    try {
      probed = spec.browser_login
        ? await invoke("probe_browser", { provider: providerId })
        : await invoke("probe_credentials", {
            provider: providerId,
            values: collectValues(),
          });

      showIdentity(probed);
      if (!fDisplay.value.trim()) fDisplay.value = probed.display;
      fSubmit.disabled = false;
      fDisplay.focus();
    } catch (err) {
      invalidate();
      showError(String(err));
    } finally {
      fProbe.disabled = false;
      fProbe.textContent = spec.browser_login ? "브라우저로 로그인" : "자격 확인";
    }
  }

  fProbe.addEventListener("click", probe);
  fCancel.addEventListener("click", () => select(null));

  fBrowser.addEventListener("click", () => {
    if (spec?.browser_url) {
      invoke("open_url", { url: spec.browser_url }).catch((err) => showError(String(err)));
    }
  });

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!probed) return;
    showError("");

    fSubmit.disabled = true;
    try {
      await invoke("create_account", {
        account: {
          provider: providerId,
          // 이름과 만료일은 사람이 적지 않는다. 확인으로 알아낸 값 그대로 쓴다.
          slug: probed.slug,
          display: fDisplay.value.trim(),
          note: "",
          expires: probed.expires ?? "",
          scopes: probed.scopes ?? [],
          git_email: probed.git_email ?? null,
          aws_account_id: probed.aws_account_id ?? null,
          root_keys_present: probed.root_keys_present ?? null,
          root_mfa: probed.root_mfa ?? null,
          values: collectValues(),
        },
      });
      // 입력한 비밀값을 DOM 에 남기지 않는다. 새 계정은 이벤트로 다시 읽힌다.
      select({ kind: "account", ref: `${providerId}/${probed.slug}` });
    } catch (err) {
      showError(String(err));
      fSubmit.disabled = false;
    }
  });

  loadProviderForm();
}


refresh.addEventListener("click", load);
load();

showTab("env");

// 만료 알림은 계정 탭을 열지 않아도 보여야 한다.
loadAccounts();
