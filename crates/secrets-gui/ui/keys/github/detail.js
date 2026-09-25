// GitHub 배포 키 상세 — 레포 하나.
//
// 맨 위가 "어디에 쓰이나"다. 키를 재발급하거나 지우기 전에 그 키를 쓰는 곳이 먼저 보여야
// 한다. 그 아래에 용도별 키가 카드로 놓인다. 다른 기계에 키를 두는 명령은 접어 둔다 —
// 서버에는 프로젝트의 [코드 받기]가 키를 둔다.

import { button, path, span } from "../../dom.js";
import { ask, back, head, purposeField, recipe } from "../parts.js";
import { termWrite } from "../../terminal.js";
import { block, chip, slots } from "../kit.js";
import { known, select, stateText } from "../state.js";
import { githubId, projectChip, usesOf } from "../usage.js";

const { invoke } = window.__TAURI__.core;

// `~/.ssh/config` 에 적힌 호스트. 한 번만 읽어 둔다.
let hosts = null;

function knownHosts() {
  hosts ??= invoke("ssh_hosts")
    .then((found) =>
      found.map((host) => ({
        value: host.alias,
        detail: [host.address, host.user].filter(Boolean).join(" · "),
      })),
    )
    .catch(() => []);
  return hosts;
}

function whereOf(key) {
  return { repo: key.repo, purpose: key.purpose };
}

/// 이 레포를 쓰는 곳. 로컬은 git 이 금고의 어느 키를 가리키는지까지 안다.
function usageBlock(repo, uses) {
  if (!uses.length) {
    return block("쓰는 곳", {}, span("kd-empty", "이 레포를 쓰는 프로젝트가 없습니다. 프로젝트 화면에서 Git을 연결하면 여기에 보입니다."));
  }
  const list = span("kd-uses", "");
  for (const u of uses) {
    const row = span("kd-use", "");
    row.append(projectChip(u));
    if (!u.environment) {
      row.append(
        span("kd-use-where", "로컬 git"),
        u.purpose ? chip(`${u.purpose} 키로 접속`, "ok") : chip("금고 밖의 키로 접속", "warn"),
      );
    } else {
      row.append(span("kd-use-where", `서버 ${u.host ?? ""}`), span("muted small", "서버에 둔 키는 이 도구가 확인하지 않습니다"));
    }
    list.append(row);
  }
  return block("쓰는 곳", { note: `${new Set(uses.map((u) => u.project)).size}개 프로젝트` }, list);
}

// 되돌릴 수 있게 실물은 보관한다. 고르는 건 삭제 여부뿐이다.
function removeButton(key, localUses) {
  const el = document.createElement("button");
  el.type = "button";
  el.className = "danger";
  el.textContent = "삭제";

  let armed = false;
  const disarm = () => {
    armed = false;
    el.textContent = "삭제";
    el.classList.remove("armed");
  };

  el.addEventListener("click", () => {
    if (!armed) {
      armed = true;
      const warn = localUses.length ? ` — ${localUses.join(", ")}의 git이 끊깁니다` : "";
      el.textContent = (key.remote_id ? "GitHub에서도 삭제" : "삭제 확인") + warn;
      el.classList.add("armed");
      setTimeout(disarm, 5000);
      return;
    }
    disarm();
    ask("remove_key", whereOf(key)).then(() => select(null)).catch(() => {});
  });
  return el;
}

// 개인 키가 금고 밖으로 나가는 유일한 동작. 누른 자리에서 결과가 보여야 한다.
function revealButton(key) {
  const el = button("개인 키 복사", {
    onClick: async () => {
      try {
        const material = await ask("reveal_private_key", whereOf(key));
        await navigator.clipboard.writeText(material);
        el.textContent = "복사됨";
        setTimeout(() => (el.textContent = "개인 키 복사"), 1500);
      } catch {
        /* 터미널 칸에 이미 남았다 */
      }
    },
  });
  return el;
}

/// 재발급 — 새 키를 GitHub 에 올리고 옛 키를 내린다. 서버에 둔 복사본은 따로 바꿔야 해서 한 번 더 묻는다.
function rotateButton(key, onServers) {
  const el = document.createElement("button");
  el.type = "button";
  el.textContent = "재발급";
  let armed = false;
  const disarm = () => {
    armed = false;
    el.textContent = "재발급";
    el.classList.remove("armed");
  };
  el.addEventListener("click", () => {
    if (!armed) {
      armed = true;
      el.textContent = onServers ? "재발급 — 서버의 키도 다시 둬야 합니다" : "재발급 확인";
      el.classList.add("armed");
      setTimeout(disarm, 5000);
      return;
    }
    disarm();
    ask("rotate_key", whereOf(key)).catch(() => {});
  });
  return el;
}

/// 멈춘 자리에서 이어 갈 수단.
function resumeButton(key) {
  if (key.state === "local") {
    return button("GitHub 등록 다시 시도", { primary: true, onClick: () => ask("retry_registration", whereOf(key)).catch(() => {}) });
  }
  if (key.state === "rotating") {
    return button("재발급 이어서 진행", { primary: true, onClick: () => ask("rotate_key", whereOf(key)).catch(() => {}) });
  }
  return null;
}

/// 다른 기계에 키를 둘 때의 명령. 평소에는 접혀 있다.
function exportRecipe(key) {
  const box = document.createElement("details");
  box.className = "kd-fold";
  const summary = document.createElement("summary");
  summary.textContent = "다른 기계에 이 키를 두는 명령";
  box.append(
    summary,
    recipe(
      [
        {
          id: `k-host-${key.purpose}`,
          label: "호스트",
          placeholder: "tukapp-prod 또는 10.0.0.1",
          fallback: "<호스트>",
          choices: { title: "호스트 고르기", load: knownHosts },
        },
        { id: `k-remote-${key.purpose}`, label: "리포 경로", placeholder: "/srv/tuk-gateway", fallback: "<리포>" },
      ],
      ([host, remote]) =>
        `scp ${key.path}/key ${host}:~/.ssh/${key.purpose}\n` +
        `ssh ${host} git -C ${remote} config core.sshCommand \\\n` +
        `  "ssh -i ~/.ssh/${key.purpose} -o IdentitiesOnly=yes"`,
    ),
  );
  return box;
}

/// 키 하나 — 용도 · 권한 · 상태가 머리에, 버튼이 오른쪽에.
function keyCard(key, uses) {
  const localUses = uses.filter((u) => !u.environment && u.purpose === key.purpose).map((u) => u.project);
  const card = document.createElement("article");
  card.className = "kd-card" + (key.state === "registered" ? "" : " warn");

  const title = span("kd-card-title", "");
  title.append(
    span("kd-card-name mono", key.purpose),
    chip(key.write ? "읽기 · 쓰기 ↓↑" : "읽기 전용 ↓"),
    chip(stateText(key), key.state === "registered" ? "ok" : "warn"),
  );
  const tools = span("kd-card-tools", "");
  const resume = resumeButton(key);
  if (resume) tools.append(resume);
  tools.append(revealButton(key), rotateButton(key, !key.write && serverEnvs.length > 0), removeButton(key, localUses));

  const serverEnvs = uses.filter((u) => u.environment).map((u) => `${u.project} · ${u.environment}`);
  const where = [];
  if (localUses.length) where.push(`${localUses.join(", ")} — 로컬 git`);
  if (!key.write && serverEnvs.length) where.push(`서버 ${serverEnvs.join(", ")}에 있을 수 있음 (서버의 키는 확인하지 않음)`);
  const facts = slots([
    ["쓰는 곳", where.length ? span("", where.join(" · ")) : span("muted", "로컬에서 이 키를 쓰는 프로젝트 없음")],
    ["용도 이름", purposeField(key.purpose, (to) => ask("set_purpose", { ...whereOf(key), to }))],
    ["지문", span("mono small", key.fingerprint)],
    ["GitHub", span("", `${key.account} · ${key.remote_id ? "id " + key.remote_id : "등록 안 됨"}${key.registered_at ? " · " + key.registered_at.slice(0, 10) : ""}`)],
    ["파일", path(`${key.path}/key`)],
  ]);
  const top = span("kd-card-head", "");
  top.append(title, tools);
  card.append(top, facts, exportRecipe(key));
  return card;
}

export function renderRepo(mount, repo) {
  const keys = known()
    .filter((k) => k.repo === repo)
    .sort((a, b) => a.purpose.localeCompare(b.purpose));
  const uses = usesOf(githubId(repo));
  const body = document.createElement("div");
  body.className = "kd-body";
  body.append(
    usageBlock(repo, uses),
    block("키", { note: `${keys.length}개 · 용도마다 하나` }, ...keys.map((k) => keyCard(k, uses))),
  );
  const [owner, ...name] = repo.split("/");
  mount.replaceChildren(back("GitHub 배포 키"), head(name.join("/"), `${owner} · GitHub 레포`, {}), body);
}

export function renderUnowned(mount, orphan) {
  const remove = document.createElement("button");
  remove.type = "button";
  remove.className = "danger";
  remove.textContent = "GitHub에서 삭제";
  remove.addEventListener("click", () => termWrite("err", "아직 만들지 않았습니다"));

  const body = document.createElement("div");
  body.className = "kd-body";
  body.append(
    block(
      "GitHub 등록",
      { note: "이 금고에 개인 키가 없습니다" },
      slots([
        ["계정", span("mono", orphan.account)],
        ["등록 위치", orphan.repo ?? "계정 전체"],
        ["지문", span("mono small", orphan.fingerprint)],
        ["원격 id", span("mono", orphan.remote_id)],
        ["등록일", orphan.registered_at ?? "모름"],
      ]),
    ),
    span("kd-empty", "이 키의 개인 키는 이 금고에 없습니다. 쓰지 않는 키라면 GitHub의 레포 설정에서 지우세요."),
  );

  mount.replaceChildren(
    back("GitHub 배포 키"),
    head(orphan.title, `${orphan.account} · ${orphan.repo ?? "계정 키"}`, { buttons: [remove] }),
    body,
  );
}
