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

const rail = document.getElementById("rail");
const detail = document.getElementById("detail");
const formTemplate = document.getElementById("tpl-form");

const PROVIDERS = [
  { id: "github", label: "GitHub" },
  { id: "aws", label: "AWS" },
  { id: "gcloud", label: "Google Cloud" },
  { id: "firebase", label: "Firebase" },
];

const OWNER_LABEL = {
  self: "내 소유",
  external: "외부 조직에서 받음",
  unknown: "소유 미확인",
};

// 지금 화면에 띄운 것. {kind: "account", ref} | {kind: "new", provider} | null
let selection = null;
let accounts = [];

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
    String(selection?.kind === "account" && selection.ref === refOf(acc)),
  );

  // 검증된 적 없으면 회색, 실패했으면 주황. 문제를 레일에서 바로 본다.
  const state = acc.verified_ok === true ? "" : acc.verified_ok === false ? " warn" : " unknown";
  button.append(span(`dot${state}`, ""));
  button.append(span("slug", acc.slug));
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

  rail.append(document.createElement("div")).className = "rail-filler";
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

function renderAccount(acc) {
  const provider = PROVIDERS.find((p) => p.id === acc.provider);

  const head = document.createElement("div");
  head.className = "detail-head";

  const titleWrap = document.createElement("div");
  titleWrap.className = "detail-title-wrap";
  titleWrap.append(span("cap", provider?.label ?? acc.provider));
  const h2 = document.createElement("h2");
  h2.textContent = acc.slug;
  titleWrap.append(h2);
  if (acc.display) titleWrap.append(span("detail-sub", acc.display));
  head.append(titleWrap);

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
      facts([
        ["로그인", acc.identity_name || "미확인", true],
        ["방식", acc.identity_kind || "—"],
        ["소유", OWNER_LABEL[acc.owner] ?? acc.owner],
        ["검증", verified],
      ]),
    ),
  );

  const note = document.createElement("p");
  note.className = "pane-note";
  note.textContent =
    "이 계정의 CLI 설정은 아래 디렉토리에만 기록됩니다. 다른 계정이나 시스템 기본 설정과 섞이지 않습니다.";

  body.append(pane("격리", note, facts([["설정 홈", acc.cli_home, true]])));

  const actions = document.createElement("div");
  actions.className = "detail-actions";
  actions.append(
    button("다시 검증", {
      primary: true,
      onClick: () => invoke("verify_account", { provider: acc.provider, slug: acc.slug }),
    }),
  );

  detail.replaceChildren(head, body, actions);
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

  if (selection?.kind === "account") {
    const acc = accounts.find((a) => refOf(a) === selection.ref);
    if (acc) return renderAccount(acc);
    selection = null;
  }
  renderEmpty();
}

function select(next) {
  selection = next;
  renderRail();
  renderDetail();
}

async function loadAccounts() {
  try {
    const result = await invoke("list_accounts");
    accounts = result.accounts;

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

/* ── 계정 추가 폼 ───────────────────────────────────── */

function bindForm(form, providerId) {
  const fProvider = form.querySelector("#f-provider");
  const fOwner = form.querySelector("#f-owner");
  const fSlug = form.querySelector("#f-slug");
  const fDisplay = form.querySelector("#f-display");
  const fGuidance = form.querySelector("#f-guidance");
  const fFields = form.querySelector("#f-fields");
  const fBrowser = form.querySelector("#f-browser");
  const fSubmit = form.querySelector("#f-submit");
  const fCancel = form.querySelector("#f-cancel");
  const fError = form.querySelector("#f-error");

  let spec = null;
  fProvider.value = providerId;

  function showError(message) {
    fError.textContent = message;
    fError.hidden = !message;
  }

  async function loadProviderForm() {
    showError("");
    fFields.replaceChildren();

    try {
      spec = await invoke("provider_form", { provider: fProvider.value });
    } catch (err) {
      showError(String(err));
      return;
    }

    fGuidance.textContent = spec.guidance;

    // CLI 가 없으면 연결이 불가능하다. 폼은 채우게 두되 미리 알린다.
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
      wrap.append(input);

      if (field.help) wrap.append(span("field-help", field.help));
      fFields.append(wrap);
    }

    fBrowser.hidden = !spec.browser_url;
    if (spec.browser_url) fBrowser.textContent = spec.browser_label;

    // 입력할 값이 없는 provider 는 아직 연결 수단이 없다.
    fSubmit.disabled = spec.fields.length === 0;
  }

  fProvider.addEventListener("change", () => {
    selection = { kind: "new", provider: fProvider.value };
    loadProviderForm();
  });

  fCancel.addEventListener("click", () => select(null));

  fBrowser.addEventListener("click", () => {
    if (spec?.browser_url) {
      invoke("open_url", { url: spec.browser_url }).catch((err) => showError(String(err)));
    }
  });

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    showError("");

    const values = {};
    for (const input of fFields.querySelectorAll("input")) {
      values[input.dataset.key] = input.value;
    }

    fSubmit.disabled = true;
    try {
      await invoke("create_account", {
        provider: fProvider.value,
        slug: fSlug.value.trim(),
        display: fDisplay.value.trim(),
        owner: fOwner.value,
        note: "",
        values,
      });
      // 입력한 비밀값을 DOM 에 남기지 않는다. 새 계정은 이벤트로 다시 읽힌다.
      select({ kind: "account", ref: `${fProvider.value}/${fSlug.value.trim()}` });
    } catch (err) {
      showError(String(err));
      fSubmit.disabled = false;
    }
  });

  loadProviderForm();
  fSlug.focus();
}


refresh.addEventListener("click", load);
load();

showTab("env");
