// 프로젝트 상세 — 로컬 · Git · 서버 · 자격 증명, 네 섹션을 위에서 아래로.
//
// 이 화면에서 자격 증명은 만들고 등록만 한다. 제거는 자격 증명 화면에서 한다.

import { span } from "../dom.js";
import { openDeployScript } from "./deploy.js";
import { openEnvironmentEdit, openProjectEdit } from "./edit.js";
import { openCredentialLink } from "./credential.js";
import { openEnv } from "./env.js";
import { openGit } from "./git.js";
import { openPull } from "./pull.js";
import { openRelease } from "./release.js";
import { checkoutLine, openServer } from "./server.js";

const { invoke } = window.__TAURI__.core;

const ROLE_TEXT = {
  example: "예시",
  local: "local",
  other: "규칙 밖 이름",
};

function header(project, onBack, onRefresh, onEdit) {
  const box = document.createElement("div");
  box.className = "project-head";

  const back = document.createElement("button");
  back.type = "button";
  back.className = "back";
  back.textContent = `← 프로젝트 / ${project.group}`;
  back.addEventListener("click", onBack);

  const h1 = document.createElement("h1");
  h1.className = "mono";
  h1.textContent = project.name;

  // 로컬 디렉토리와 git 상태를 다시 읽는다. 서버는 환경마다 [서버 확인]으로 읽는다.
  const status = span("muted small", "");
  const refresh = action("새로고침", {
    onClick: async () => {
      refresh.disabled = true;
      status.textContent = "다시 읽는 중…";
      status.className = "muted small";
      try {
        await onRefresh();
      } catch (err) {
        status.textContent = String(err);
        status.className = "warn-text small";
        refresh.disabled = false;
      }
    },
  });
  refresh.title = "로컬 디렉토리와 git 상태를 다시 읽습니다";

  const title = document.createElement("div");
  title.className = "project-title-row";
  const tools = document.createElement("div");
  tools.className = "row-actions";
  const editButton = action("편집", { onClick: onEdit });
  tools.append(status, refresh, editButton);
  title.append(h1, tools);
  box.append(back, title);
  return box;
}

/// 버튼 하나. `reason` 이 있으면 누를 수 없고 이유를 툴팁으로 보여 준다.
function action(label, { onClick, reason } = {}) {
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = label;
  if (reason) {
    button.disabled = true;
    button.title = reason;
  } else {
    button.addEventListener("click", onClick);
  }
  return button;
}

/// 섹션 하나 — 제목 줄(오른쪽에 그 섹션의 버튼)과 본문.
function section(title, buttons, ...children) {
  const box = document.createElement("section");
  box.className = "project-pane detail-section";
  const head = document.createElement("div");
  head.className = "detail-section-head";
  const h2 = document.createElement("h2");
  h2.textContent = title;
  const actions = document.createElement("div");
  actions.className = "row-actions";
  actions.append(...buttons);
  head.append(h2, actions);
  box.append(head, ...children.filter(Boolean));
  return box;
}

/// 태그 모양의 짧은 상태. `tone` 은 ok · warn · 빈 값.
function chip(text, tone = "") {
  return span(tone ? `chip ${tone}` : "chip", text);
}

/// 요소 하나를 만들고 자식을 붙인다.
function el(tag, className, ...children) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  node.append(...children.filter((c) => c !== null && c !== undefined));
  return node;
}

/// 이름 칸과 값 칸이 한 줄인 목록. 값은 노드 여러 개일 수 있다.
function slots(rows) {
  const list = el("dl", "slots");
  for (const [label, ...value] of rows) list.append(el("dt", "", label), el("dd", "", ...value));
  return list;
}

function table(heads, rows) {
  const head = el("tr", "", ...heads.map((h) => el("th", "", h)));
  const body = el("tbody", "", ...rows.map((cells) => el("tr", "", ...cells.map((c) => el("td", "", c)))));
  return el("div", "table-scroll", el("table", "detail-table", el("thead", "", head), body));
}

/* ── 로컬 ───────────────────────────────────────────── */

function runtimeChips(runtimes) {
  if (!runtimes.length) return [span("muted small", "런타임을 감지하지 못했습니다")];
  return runtimes.map((r) => {
    const c = chip(r.version ? `${r.label} ${r.version}` : r.label, r.conflict ? "warn" : "");
    c.title = `근거 ${r.sources.join(", ")}${r.conflict ? " · 근거끼리 버전이 다름" : ""}`;
    return c;
  });
}

/// git 이 이 파일을 어떻게 보는가. 값이 원격에 올라갈 수 있으면 경고로.
function gitChip(file) {
  if (file.role === "example") return chip(file.tracked ? "추적됨" : "추적 안 함");
  if (file.tracked) return chip("추적됨 · 값이 올라갈 수 있음", "warn");
  if (file.ignored === true) return chip("제외됨");
  if (file.ignored === false) return chip(".gitignore에 없음", "warn");
  return span("muted small", "git 없음");
}

function envFileTable(project) {
  const files = project.scan.env_files;
  if (!files.length) return span("muted small", "환경 변수 파일이 없습니다.");
  return table(
    ["파일", "환경", "변수", "서버로", "git"],
    files.map((file) => {
      const env = file.role === "environment" ? file.env : ROLE_TEXT[file.role];
      const targets = project.environments.filter((e) => e.env_file === file.name);
      const where = targets.length
        ? el("span", "chips", ...targets.map((e) => chip(`${e.name} → ${e.server_env_file}`, "ok")))
        : span("muted small", "—");
      return [
        span("mono strong", file.name),
        span(file.role === "example" ? "muted" : "", env),
        span("num", String(file.variables)),
        where,
        gitChip(file),
      ];
    }),
  );
}

function localSection(project) {
  if (project.scan.error) {
    return section(
      "로컬",
      [],
      slots([["경로", span("mono", project.path)]]),
      span("notice warn", `${project.scan.error} 디렉토리를 옮겼다면 기록의 경로와 맞지 않는 상태입니다.`),
    );
  }
  const exposed = project.scan.env_files.filter((f) => f.exposed);
  return section(
    "로컬",
    [],
    slots([
      ["경로", span("mono", project.path)],
      ["런타임", el("span", "chips", ...runtimeChips(project.scan.runtimes))],
    ]),
    el(
      "div",
      "sub-block",
      el("div", "sub-head", span("sub-title", "환경 변수 파일"), span("muted small", "로컬이 정본 · 이름만 읽고 값은 읽지 않음")),
      envFileTable(project),
    ),
    exposed.length
      ? span(
          "notice warn",
          `${exposed.map((f) => f.name).join(", ")}의 값이 원격 레포에 올라갈 수 있습니다. Git을 연결하기 전에 .gitignore에 추가하세요.`,
        )
      : null,
    span(
      "section-foot",
      `${project.origin === "created" ? "새로 만든 프로젝트" : "기존 디렉토리 등록"} · ${project.created_at}`,
    ),
  );
}

/* ── Git ────────────────────────────────────────────── */

/// 이 레포가 어떤 SSH 키로 GitHub 에 접속하는가.
function keyChip(git) {
  if (!git.ssh_key) return chip("키 지정 없음 · 계정 기본 키");
  // ~/.secrets/keys/github/repo/<소유자>/<레포>/<용도>/key
  const inVault = git.ssh_key.includes("/.secrets/keys/github/");
  const c = chip(inVault ? `레포 전용 키 · ${git.ssh_key.split("/").at(-2)}` : "다른 키", inVault ? "" : "warn");
  c.title = git.ssh_key;
  return c;
}

function gitSection(project) {
  const git = project.scan.git;
  if (project.scan.error || !git) return section("Git", [], span("muted small", "로컬 디렉토리를 읽지 못했습니다."));
  if (git.kind !== "remote") {
    const state = git.kind === "absent" ? "git 저장소가 아닙니다." : "로컬 git만 있고 원격 레포가 없습니다.";
    return section(
      "Git",
      [action("Git 연결", { onClick: () => openGit(project) })],
      span("muted", `${state} 코드를 올릴 준비가 되면 연결합니다.`),
    );
  }
  return section(
    "Git",
    [action("키 · 연결 확인", { onClick: () => openGit(project) })],
    slots([
      ["레포", span("mono strong", git.repo ?? git.origin)],
      [
        "로컬",
        el(
          "span",
          "chips",
          chip(git.branch ? `브랜치 ${git.branch}` : "분리된 HEAD", git.branch ? "" : "warn"),
          git.changes ? chip(`커밋 안 한 변경 ${git.changes}`, "warn") : chip("변경 없음", "ok"),
        ),
      ],
      ["접속", keyChip(git)],
    ]),
  );
}

/* ── 서버 ───────────────────────────────────────────── */

/// 배포할 수 있는가 — 기록만 보고 판단한다. 서버와 같은지는 배포 창에서 확인한다.
function readiness(env) {
  if (!env.deploy_script) return chip("배포 스크립트 없음", "warn");
  if (!env.env_file) return chip("환경 변수 파일 고르지 않음", "warn");
  return chip("배포 준비됨", "ok");
}

/// 환경 하나를 카드로. 서버는 [서버 확인]을 누를 때만 읽는다.
function environmentCard(project, env) {
  const release = action("배포", { onClick: () => openRelease(project, env.name) });
  release.className = "primary";

  const checked = el("div", "env-checked");
  checked.hidden = true;
  const check = action("서버 확인", {
    onClick: async () => {
      check.disabled = true;
      checked.hidden = false;
      checked.className = "env-checked";
      checked.replaceChildren(span("muted small", "서버를 읽는 중…"));
      try {
        const found = await invoke("check_environment", { project: project.name, environment: env.name });
        const ok = found.state === "repository";
        checked.className = "env-checked " + (ok ? "ok" : "warn");
        const rows = [["배포 경로", span("", checkoutLine(found))]];
        if (found.owner) rows.push(["소유", span("mono", `${found.owner}:${found.group ?? ""}`)]);
        if (found.ssh_command) rows.push(["git 키", span("mono small", found.ssh_command)]);
        checked.replaceChildren(slots(rows));
      } catch (err) {
        checked.className = "env-checked warn";
        checked.replaceChildren(span("warn-text small", String(err)));
      } finally {
        check.disabled = false;
      }
    },
  });
  const tools = [
    check,
    action("코드 받기", { onClick: () => openPull(project, env.name) }),
    action("환경 변수", { onClick: () => openEnv(project, env.name) }),
    action("배포 스크립트", { onClick: () => openDeployScript(project, env.name) }),
    action("편집", { onClick: () => openEnvironmentEdit(project, env.name) }),
  ];
  for (const b of tools) b.classList.add("small-button");

  const head = el(
    "div",
    "env-card-head",
    el(
      "div",
      "env-card-title",
      span("env-name", env.name),
      el("div", "env-card-where", span("strong", env.instance_name || env.instance), span("muted small", `${env.login} 계정`)),
    ),
    el("div", "env-card-cta", readiness(env), release),
  );
  const facts = slots([
    ["서버", span("mono", `${env.login}@${env.address}:${env.path}`)],
    ["배포 대상", span("mono", `origin/${env.branch || "—"}`)],
    [
      "환경 변수",
      env.env_file
        ? span("mono", `${env.env_file} → ${env.server_env_file}`)
        : span("warn-text small", "고르지 않음 — [환경 변수]에서 고릅니다"),
    ],
    [
      "배포 스크립트",
      env.deploy_script ? span("", "있음") : span("warn-text small", "없음 — [배포 스크립트]에서 작성합니다"),
    ],
  ]);
  return el(
    "article",
    "env-card",
    head,
    facts,
    checked,
    el("div", "env-card-foot", el("div", "row-actions", ...tools), span("muted small", `연결 ${env.connected_at}`)),
  );
}

function serverSection(project) {
  const gitReady = project.scan.git?.kind === "remote";
  const label = project.environments.length ? "＋ 환경 추가" : "서버 연결";
  const button = gitReady
    ? action(label, { onClick: () => openServer(project) })
    : action(label, { reason: "Git을 먼저 연결하세요. 서버는 GitHub에서 코드를 받습니다." });
  if (!project.environments.length) {
    const note = gitReady
      ? "연결된 서버가 없습니다."
      : "연결된 서버가 없습니다. 서버는 GitHub에서 코드를 받으므로 Git을 먼저 연결하세요.";
    return section("서버", [button], span("muted", note));
  }
  return section("서버", [button], el("div", "env-cards", ...project.environments.map((env) => environmentCard(project, env))));
}

/* ── 자격 증명 ──────────────────────────────────────── */

/// 자격 증명 하나가 놓인 곳들을 한 줄로 모으고, 환경별로 묶는다.
function credentialGroups(linked) {
  const byName = new Map();
  for (const c of linked) {
    const key = c.kind + "|" + c.name;
    if (!byName.has(key)) byName.set(key, { ...c, places: [] });
    byName.get(key).places.push(c);
  }
  const groups = new Map();
  for (const cred of byName.values()) {
    const envs = [...new Set(cred.places.map((p) => p.environment).filter(Boolean))].sort();
    const group = envs.length ? envs.join(" · ") : "환경 없음";
    if (!groups.has(group)) groups.set(group, []);
    groups.get(group).push(cred);
  }
  const order = [...groups.keys()].sort((a, b) => (a === "환경 없음") - (b === "환경 없음") || a.localeCompare(b));
  return el(
    "div",
    "cred-groups",
    ...order.map((group) =>
      el(
        "div",
        "cred-group",
        el(
          "div",
          "cred-group-head",
          group === "환경 없음" ? span("muted small strong", "환경에 묶이지 않은 파일") : span("env-name small", group),
          span("muted small", `${groups.get(group).length}개`),
        ),
        ...groups.get(group).map((cred) => {
          const where = cred.places.map((p) => {
            const c = chip(`${p.host ? p.host : "로컬"} · ${p.file}`, p.host ? "edge" : "");
            c.title = p.host ? `서버 ${p.host}의 배포 경로 안 ${p.file}` : `로컬 프로젝트 안 ${p.file}`;
            return c;
          });
          const name = span("strong mono", cred.name);
          const purpose = cred.purpose ? span("muted small cred-purpose", cred.purpose) : null;
          if (purpose) purpose.title = cred.purpose;
          return el(
            "div",
            "cred-row",
            el("div", "cred-main", name, purpose),
            el("div", "cred-side", el("span", "chips", ...where), span("mono small muted", cred.variable ?? cred.detail)),
          );
        }),
      ),
    ),
  );
}

function credentialSection(project) {
  const list = el("div", "", span("muted small", "자격 증명 기록을 읽는 중…"));
  const button = action("＋ 자격 증명 연결", { reason: "자격 증명 기록을 읽는 중입니다." });

  async function fill() {
    try {
      const found = await invoke("project_credentials", { project: project.name });
      button.disabled = false;
      button.title = "";
      button.onclick = () => openCredentialLink(project, found, fill);
      if (!found.linked.length) {
        list.replaceChildren(span("muted", "이 프로젝트나 연결된 서버의 파일을 가리키는 자격 증명 기록이 없습니다."));
        return;
      }
      list.replaceChildren(credentialGroups(found.linked));
    } catch (err) {
      list.replaceChildren(span("warn-text small", String(err)));
    }
  }
  fill();

  return section(
    "자격 증명",
    [button],
    list,
    span("section-foot", "이미 있는 자격 증명을 어느 파일에 넣었는지 기록합니다. 발급과 제거는 자격 증명 화면에서 합니다."),
  );
}

export function renderDetail(mount, project, { onBack, onRefresh, notices, groups, onRenamed, onUnregistered }) {
  const body = document.createElement("div");
  body.className = "detail-sections";
  for (const message of notices) body.append(span("notice warn", message));
  body.append(localSection(project), gitSection(project), serverSection(project), credentialSection(project));
  const onEdit = () => openProjectEdit(project, { groups, onRenamed, onUnregistered });
  mount.replaceChildren(header(project, onBack, onRefresh, onEdit), body);
}
