// GitHub 배포 키 상세.

import { button, facts, pane, path, span } from "../../dom.js";
import { ask, back, command, head, purposeField, recipe, side } from "../parts.js";
import { termWrite } from "../../terminal.js";
import { permissionText, select, stateText } from "../state.js";

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

function filesPane(key) {
  return pane(
    "파일",
    facts([
      ["개인 키", path(`${key.path}/key`), true],
      ["공개 키", path(`${key.path}/key.pub`), true],
      ["지문", key.fingerprint, true],
      ["알고리즘", key.algorithm],
      ["만든 날", key.created_at],
    ]),
  );
}

function registrationPane(key) {
  const rows = [
    [
      "용도",
      purposeField(key.purpose, (to) => ask("set_purpose", { ...whereOf(key), to })),
    ],
    ["계정", key.account, true],
    ["권한", permissionText(key)],
    ["상태", stateText(key)],
  ];
  if (key.remote_id) rows.push(["원격 id", key.remote_id, true]);
  if (key.registered_at) rows.push(["등록일", key.registered_at]);

  const box = pane("GitHub 등록", facts(rows));

  // 멈춘 자리에서 이어 갈 수단만 붙인다.
  if (key.state === "local") {
    box.append(
      button("등록 다시 시도", {
        primary: true,
        onClick: () => ask("retry_registration", whereOf(key)).catch(() => {}),
      }),
    );
  }
  if (key.state === "rotating") {
    box.append(
      button("이어서 진행", {
        primary: true,
        onClick: () => ask("rotate_key", whereOf(key)).catch(() => {}),
      }),
    );
  }
  return box;
}

// 개인 키가 금고 밖으로 나가는 유일한 자리.
//
// 이 맥에서 쓰는 키는 나갈 일이 없다 — 리포의 core.sshCommand 가 금고를 바로
// 가리킨다. 나가는 것은 다른 기계가 쓸 키뿐이다.
function exportPane(key) {
  return pane(
    "내보내기",
    facts([["개인 키", path(`${key.path}/key`), true]]),
    revealButton(key),
    span("subhead", "서버 · 에이전트"),
    recipe(
      [
        {
          id: "k-host",
          label: "호스트",
          placeholder: "tukapp-prod 또는 10.0.0.1",
          fallback: "<호스트>",
          choices: { title: "호스트 고르기", load: knownHosts },
        },
        { id: "k-remote", label: "리포 경로", placeholder: "~/apps/tuk-gateway", fallback: "<리포>" },
      ],
      ([host, remote]) =>
        `scp ${key.path}/key ${host}:~/.ssh/${key.purpose}\n` +
        `ssh ${host} git -C ${remote} config core.sshCommand \\\n` +
        `  "ssh -i ~/.ssh/${key.purpose} -o IdentitiesOnly=yes"`,
    ),
  );
}

export function renderKey(mount, key) {
  const badges = [];
  if (key.state !== "registered") badges.push(span("badge-warn", stateText(key)));

  const body = document.createElement("div");
  body.className = "detail-body";
  body.append(side(filesPane(key), registrationPane(key)), exportPane(key));

  mount.replaceChildren(
    back("배포 키"),
    head(key.repo, null, {
      badges,
      buttons: [
        button("재발급", { onClick: () => ask("rotate_key", whereOf(key)).catch(() => {}) }),
        removeButton(key),
      ],
    }),
    body,
  );
}

// 되돌릴 수 있게 실물은 보관한다. 고르는 건 삭제 여부뿐이다.

// 되돌릴 수 있게 실물은 보관한다. 고르는 건 삭제 여부뿐이다.
function removeButton(key) {
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
      el.textContent = key.remote_id ? "GitHub 에서도 지우고 삭제" : "삭제 확인";
      el.classList.add("armed");
      setTimeout(disarm, 4000);
      return;
    }
    disarm();
    ask("remove_key", whereOf(key)).then(() => select(null)).catch(() => {});
  });
  return el;
}

// 개인 키가 금고 밖으로 나가는 유일한 동작. 누른 자리에서 결과가 보여야 한다.

// 개인 키가 금고 밖으로 나가는 유일한 동작. 누른 자리에서 결과가 보여야 한다.
function revealButton(key) {
  const el = button("개인 키 내용 복사", {
    onClick: async () => {
      try {
        const material = await ask("reveal_private_key", whereOf(key));
        await navigator.clipboard.writeText(material);
        el.textContent = "복사됨";
        setTimeout(() => (el.textContent = "개인 키 내용 복사"), 1500);
      } catch {
        /* 터미널 칸에 이미 남았다 */
      }
    },
  });
  return el;
}

export function renderUnowned(mount, orphan) {
  const remove = document.createElement("button");
  remove.type = "button";
  remove.className = "danger";
  remove.textContent = "GitHub 에서 삭제";
  remove.addEventListener("click", () => termWrite("err", "아직 만들지 않았습니다"));

  const body = document.createElement("div");
  body.className = "detail-body";
  body.append(
    pane(
      "GitHub 등록",
      facts([
        ["계정", orphan.account, true],
        ["자리", orphan.repo ?? "계정 전체"],
        ["지문", orphan.fingerprint, true],
        ["원격 id", orphan.remote_id, true],
        ["등록일", orphan.registered_at ?? "모름"],
      ]),
    ),
  );

  mount.replaceChildren(
    back("개인 키 없음"),
    head(orphan.title, `${orphan.account} · ${orphan.repo ?? "계정 키"}`, { buttons: [remove] }),
    body,
  );
}
