// 배포 — 로컬 · 원격 · 서버의 커밋을 견주어 보여 주고, [배포]를 누르면 배포 스크립트를 돌린다.
//
// 코드가 서로 달라도 막지 않는다 — 경고만 한다. 막는 것은 배포 스크립트가 없을 때와 환경 변수
// 파일이 로컬과 서버에서 다를 때뿐이다. 스크립트 출력은 이 창과 작업 로그에 함께 흐른다.

import { span } from "../dom.js";
import { modal } from "../modal.js";
import { invoke } from "../ipc.js";

const { listen } = window.__TAURI__.event;

function revisionText(rev) {
  return rev ? rev.sha + "  " + rev.subject : "—";
}

function el(tag, className, ...children) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  node.append(...children);
  return node;
}

/// 맨 위 결론 한 줄 — 배포할 수 있는가, 경고가 몇 개인가.
function verdict(plan) {
  const warns = plan.notes.filter((n) => n.tone === "warn").length;
  let tone, head;
  if (plan.blockers.length) {
    tone = "blocked";
    head = "배포할 수 없습니다";
  } else if (plan.same) {
    tone = "ok";
    head = "배포할 수 있습니다 · 로컬 · 원격 · 서버가 같은 커밋입니다";
  } else if (warns) {
    tone = "warn";
    head = "배포할 수 있습니다 · 경고 " + warns + "개";
  } else {
    tone = "ok";
    head = "배포할 수 있습니다";
  }
  const box = el("div", "release-verdict " + tone, span("release-verdict-head", head));
  for (const reason of plan.blockers) box.append(span("release-verdict-line", reason));
  return box;
}

/// 지금 서버 → 배포 후. 배포되는 것은 원격의 커밋이다.
function change(plan) {
  const side = (label, rev) =>
    el(
      "div",
      "release-side",
      span("release-side-label", label),
      span("release-sha mono", rev ? rev.sha : "—"),
      span("release-subject", rev ? rev.subject : ""),
    );
  let note;
  if (plan.incoming > 0) note = "새 커밋 " + plan.incoming + "개가 서버에 들어갑니다";
  else if (plan.incoming === 0) note = "서버에 이미 이 커밋이 있습니다 — 스크립트만 다시 실행합니다";
  else note = "서버의 커밋과 견주지 못했습니다";
  return el(
    "div",
    "release-change",
    el("div", "release-flow", side("지금 서버", plan.server), span("release-arrow", "→"), side("배포 후 (origin/" + plan.branch + ")", plan.remote)),
    span("release-change-note", note),
  );
}

/// 로컬 · 원격 · 서버를 한 표로. 마지막 칸은 원격과 견준 관계.
function commitTable(plan) {
  const table = el("table", "release-table");
  const head = el("tr", "");
  for (const text of ["", "커밋", "제목", "원격과"]) head.append(el("th", "", text));
  table.append(el("thead", "", head));
  const body = el("tbody", "");
  for (const [label, rev, relation] of [
    ["로컬 " + plan.branch, plan.local, plan.local_relation],
    ["원격", plan.remote, "기준"],
    ["서버", plan.server, plan.server_relation],
  ]) {
    const tone = relation === "같음" || relation === "기준" ? "same" : "differ";
    const row = el("tr", "");
    row.append(
      el("th", "", label),
      el("td", "mono", rev ? rev.sha : "—"),
      el("td", "release-subject-cell", rev ? rev.subject : ""),
      el("td", "", span("release-rel " + tone, relation)),
    );
    body.append(row);
  }
  table.append(body);
  return el("div", "table-scroll", table);
}

/// 경고와 참고. 코드가 달라도 배포는 막지 않는다.
function notes(plan) {
  if (!plan.notes.length) return null;
  const warns = plan.notes.filter((n) => n.tone === "warn");
  const infos = plan.notes.filter((n) => n.tone !== "warn");
  const list = el("ul", "release-notes");
  for (const n of warns) list.append(el("li", "warn", n.text));
  for (const n of infos) list.append(el("li", "info", n.text));
  return el(
    "div",
    "release-block",
    span("release-block-head", warns.length ? "경고 " + warns.length : "참고"),
    list,
    span("muted small", "코드가 달라도 배포할 수 있습니다. 배포되는 것은 원격 origin/" + plan.branch + "입니다."),
  );
}

/// 배포를 막는 두 가지 — 환경 변수, 배포 스크립트.
function checks(plan) {
  const row = (ok, label, text) =>
    el("li", ok ? "ok" : "bad", span("release-mark", ok ? "✓" : "✗"), span("release-check-label", label), span("release-check-text", text));
  const envOk = !plan.env || plan.env.state === "same";
  const envText = plan.env
    ? (plan.env.state === "same" ? "일치 — " : "다름 — ") + plan.env.local_file + " → " + plan.env.server_file
    : "고른 파일이 없어 비교하지 않음";
  const list = el("ul", "release-checks", row(envOk, "환경 변수", envText), row(Boolean(plan.script), "배포 스크립트", plan.script || "없음"));
  return el("div", "release-block", span("release-block-head", "배포 전 점검"), list);
}

/* ── 배포 중 · 뒤 ─────────────────────────────────────── */

// 도구가 나누는 단계. 서버 쪽 스크립트 안의 단계는 스크립트가 `## 제목` 줄로 적는다.
const STEP_NAMES = ["환경 변수 비교", "서버의 지금 커밋 읽기", "배포 스크립트 실행", "배포 결과 읽기"];

// 줄의 무게. 결과를 가르는 줄은 드러내고, 도구가 부르는 명령 · 설치 도구의 안내는 흐리게.
const NOISE = [
  /^\s*$/,
  /^[╭╰│┌├└─┐┘┤┬┴┼]/,
  /^\s+[│╭╰]/,
  /Done in [\d.]/,
  /Lockfile is up to date/,
  /^Already up to date/,
  /Update available|Changelog:|To update, run/,
  /Use --update-env/,
  /Applying action \w+ProcessId/,
];
// 서버를 읽을 때 도구가 받는 `이름=값` 줄.
const PROBE_KEYS = new Set(["state", "origin", "branch", "commit", "owner", "group", "sshcommand", "tracking", "mode"]);
const GOOD = [/health ok/i, /✓/, /^배포 끝/, /되돌림 완료/];

// 도구들은 진행 상황도 표준 오류로 내보낸다(git · esbuild · pnpm). 오류로 칠하는 것은 실패를 말하는 줄만.
const FAILURE = /error|fail|fatal|denied|refused|실패|없습니다|멈춥니다|되돌립니다/i;

function lineKind(stream, text) {
  if ((stream === "err" && FAILURE.test(text)) || text.startsWith("✗ ")) return "err";
  if (text.startsWith("$ ")) return "cmd";
  if (text.startsWith("## ")) return "sub";
  if (GOOD.some((re) => re.test(text))) return "good";
  if (/^commit [0-9a-f]{7}/.test(text)) return "key";
  if (NOISE.some((re) => re.test(text)) || PROBE_KEYS.has(text.split("=")[0])) return "dim";
  return "";
}

function seconds(ms) {
  return (ms / 1000).toFixed(ms < 10000 ? 1 : 0) + "초";
}

/// 단계 표시 · 단계별 로그. 이 창이 떠 있는 동안 그 배포 job 의 출력을 받아 적는다.
function runView(label, names) {
  const track = el("ol", "run-steps");
  const items = names.map((name) => {
    const li = el("li", "pending", span("run-step-mark", ""), span("run-step-name", name), span("run-step-time", ""));
    track.append(li);
    return li;
  });
  const log = el("div", "release-log");
  let job = null;
  let index = -1;
  let block = log;
  let startedAt = 0;
  const stops = [];

  function finishStep(state) {
    if (index < 0) return;
    const li = items[index];
    li.className = state;
    li.querySelector(".run-step-time").textContent = seconds(Date.now() - startedAt);
  }
  function openStep(name) {
    finishStep("done");
    index = names.indexOf(name);
    if (index < 0) {
      names.push(name);
      items.push(el("li", "", span("run-step-mark", ""), span("run-step-name", name), span("run-step-time", "")));
      track.append(items.at(-1));
      index = names.length - 1;
    }
    items[index].className = "running";
    startedAt = Date.now();
    block = el("section", "run-block", span("run-block-head", name));
    log.append(block);
  }
  // 흐린 줄이 이어지면 한 묶음으로 접는다. 한 줄뿐이면 그대로 보인다.
  let dim = null;
  function appendDim(text) {
    if (!dim || block.lastChild !== dim.details) {
      const details = el("details", "dim-run");
      dim = { details, summary: el("summary", "run-line dim"), body: el("div", "dim-body"), count: 0, first: text };
      details.append(dim.summary, dim.body);
      block.append(details);
    }
    dim.count += 1;
    dim.body.append(el("div", "run-line dim", text));
    dim.details.classList.toggle("single", dim.count === 1);
    dim.summary.textContent = dim.count === 1 ? text : `도구 출력 ${dim.count}줄`;
  }
  function write(stream, text) {
    if (!text.trim()) return;
    const kind = lineKind(stream, text);
    const atBottom = log.scrollTop + log.clientHeight >= log.scrollHeight - 8;
    if (kind === "dim") appendDim(text);
    else block.append(el("div", "run-line " + kind, kind === "sub" ? text.slice(3) : text));
    if (atBottom) log.scrollTop = log.scrollHeight;
  }

  const alive = () => document.body.contains(log);
  const stopAll = () => stops.forEach((stop) => stop());
  listen("cli:start", (e) => {
    if (!alive()) return stopAll();
    if (e.payload.command === label) job = e.payload.job;
  }).then((stop) => stops.push(stop));
  listen("cli:line", (e) => {
    if (!alive()) return stopAll();
    if (e.payload.job !== job) return;
    if (e.payload.stream === "step") openStep(e.payload.line);
    else write(e.payload.stream, e.payload.line);
  }).then((stop) => stops.push(stop));

  return {
    track,
    log,
    /// 끝났을 때. 실패면 진행 중이던 단계를 실패로, 남은 단계는 건너뜀으로.
    end(ok) {
      stopAll();
      requestAnimationFrame(() => (log.scrollTop = log.scrollHeight));
      if (ok) {
        finishStep("done");
        return;
      }
      finishStep("failed");
      for (const li of items.slice(index + 1)) li.className = "skipped";
    },
  };
}

function body(project, env, holder, close) {
  const label = project.name + " · " + env.name + " 배포";
  let plan = null;

  // 두 화면을 번갈아 보인다 — 배포 전 점검(스크롤 하나), 배포 중 · 뒤(로그가 남은 높이를 채우고 스크롤).
  const content = document.createElement("div");
  content.className = "release-body";
  const run = document.createElement("div");
  run.className = "release-run";
  run.hidden = true;
  const result = span("", "");
  result.hidden = true;

  const problem = span("problem", "");
  problem.hidden = true;
  const shut = document.createElement("button");
  shut.type = "button";
  shut.textContent = "닫기";
  shut.addEventListener("click", close);
  const again = document.createElement("button");
  again.type = "button";
  again.textContent = "다시 확인";
  const deploy = document.createElement("button");
  deploy.type = "button";
  deploy.className = "primary";
  deploy.textContent = "배포";
  deploy.disabled = true;
  const actions = document.createElement("div");
  actions.className = "modal-actions";
  actions.append(problem, shut, again, deploy);

  async function check() {
    run.hidden = true;
    content.hidden = false;
    problem.hidden = true;
    deploy.disabled = true;
    again.disabled = true;
    content.replaceChildren(span("muted", "원격을 가져오고 서버를 읽는 중…"));
    try {
      plan = await invoke("deploy_plan", { project: project.name, environment: env.name });
      content.replaceChildren(
        ...[verdict(plan), change(plan), commitTable(plan), notes(plan), checks(plan)].filter(Boolean),
      );
      deploy.disabled = plan.blockers.length > 0;
    } catch (err) {
      content.replaceChildren();
      problem.textContent = String(err);
      problem.hidden = false;
    } finally {
      again.disabled = false;
    }
  }

  deploy.addEventListener("click", async () => {
    problem.hidden = true;
    deploy.disabled = true;
    again.disabled = true;
    shut.disabled = true;
    deploy.textContent = "배포하는 중…";
    // 점검 화면을 접고 로그에 자리를 준다.
    content.hidden = true;
    run.hidden = false;
    result.hidden = true;
    const names = STEP_NAMES.filter((n) => plan?.env || n !== "환경 변수 비교");
    const view = runView(label, names);
    const began = Date.now();
    run.replaceChildren(view.track, result, view.log);
    try {
      const done = await invoke("run_deploy", { project: project.name, environment: env.name });
      view.end(true);
      result.className = "release-verdict ok";
      const same = done.before && done.after && done.before.sha === done.after.sha;
      result.replaceChildren(
        span("release-verdict-head", "배포했습니다 · " + seconds(Date.now() - began)),
        span(
          "release-verdict-sub",
          same
            ? `서버 ${done.after.sha} — 같은 커밋을 다시 설치하고 재시작했습니다`
            : `서버 ${done.before?.sha ?? "—"} → ${revisionText(done.after)}`,
        ),
      );
    } catch (err) {
      view.end(false);
      result.className = "release-verdict blocked";
      result.replaceChildren(
        span("release-verdict-head", "배포 실패"),
        span("release-verdict-sub", String(err)),
      );
    } finally {
      result.hidden = false;
      deploy.textContent = "배포";
      shut.disabled = false;
      again.disabled = false;
    }
  });
  again.addEventListener("click", check);

  holder.append(content, run, actions);
  check();
}

/// 환경 하나의 배포 창을 연다.
export function openRelease(project, environment) {
  const env = project.environments.find((e) => e.name === environment);
  modal(
    `배포 · ${project.name} · ${environment}`,
    (close) => {
      const holder = document.createElement("div");
      holder.className = "modal-fill";
      body(project, env, holder, close);
      return [holder];
    },
    { size: "lg", fill: true },
  );
}
