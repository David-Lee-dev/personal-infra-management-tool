// IAM — 목록 칸과 상세.
//
// IAM 하나는 권한 하나다. 무엇을 할 수 있는지가 가장 먼저 보이고, 그다음이 키,
// 마지막이 그 키를 넣었다고 적은 곳이다. `.env` 에 넣고 빼는 일은 사람이 하고,
// 이 화면은 기록과 붙여 넣을 두 줄만 준다.
//
// 키를 바꾸는 버튼은 없다. 새 IAM 을 만들고 옛 것은 치운다 — 옛 것은 키가 한 달
// 넘게 쓰이지 않아야 지워진다.

import { button, facts, pane, path, span } from "../../dom.js";
import { chooser } from "../../combo.js";
import { ask, back, head, purposeField, side } from "../parts.js";
import { select } from "../state.js";
import { iamUsers } from "./state.js";
import { groupRow, row, section, table } from "../table.js";

const { invoke } = window.__TAURI__.core;

const ENV_ORDER = ["prod", "dev", "local"];
const LOCAL = "local";

function hostLabel(host) {
  return host === LOCAL ? "이 맥" : host;
}

function whereOf(user) {
  return { account: user.account, name: user.name };
}

function mono(text) {
  const el = span("mono", text);
  el.title = text;
  return el;
}

// 누르면 한 번 더 묻는다. 되돌릴 수 없는 일에만 쓴다.
function armed(label, confirm, action, { danger = false } = {}) {
  const el = document.createElement("button");
  el.type = "button";
  el.textContent = label;
  if (danger) el.className = "danger";

  let ready = false;
  const disarm = () => {
    ready = false;
    el.textContent = label;
    el.classList.remove("armed");
  };
  el.addEventListener("click", () => {
    if (!ready) {
      ready = true;
      el.textContent = confirm;
      el.classList.add("armed");
      setTimeout(disarm, 4000);
      return;
    }
    disarm();
    el.disabled = true;
    action().finally(() => {
      el.disabled = false;
    });
  });
  return el;
}

/* ── 목록 ─────────────────────────────────────────── */

const COLUMNS = [
  { label: "IAM", width: "36%" },
  { label: "환경", width: "10%" },
  { label: "권한", width: "40%" },
  { label: "소비처", width: "14%" },
];

function scopeCell(user) {
  const box = document.createElement("div");
  box.className = "cell-actions";
  box.append(span("chip", user.service || "?"), mono(user.scope));
  return box;
}

export function iamSection() {
  const users = iamUsers();
  if (!users.length) {
    return [section("IAM", 0), span("list-none", "＋ IAM 만들기 를 눌러 시작하세요.")];
  }

  // 이름이 `<앱>-<환경>-<권한>-iam` 이라 앱이 곧 묶음이다.
  const apps = [...new Set(users.map((user) => user.app))].sort();
  const rows = [];
  for (const app of apps) {
    rows.push(groupRow(app, COLUMNS.length));
    const mine = users
      .filter((user) => user.app === app)
      .sort(
        (a, b) =>
          ENV_ORDER.indexOf(a.env) - ENV_ORDER.indexOf(b.env) || a.name.localeCompare(b.name),
      );
    for (const user of mine) {
      rows.push(
        row(
          [user.name, user.env, { node: scopeCell(user) }, `${user.consumers.length}곳`],
          () => select({ kind: "iam", ref: user.ref }),
        ),
      );
    }
  }
  return [section("IAM", users.length), table(COLUMNS, rows)];
}

/* ── 상세 ─────────────────────────────────────────── */

function rulesPane(user) {
  const box = pane("권한");
  box.append(
    table(
      [
        { label: "", width: "8%" },
        { label: "동작", width: "30%" },
        { label: "대상", width: "48%" },
        { label: "조건", width: "14%" },
      ],
      user.rules.map((rule) =>
        row([rule.effect, { node: mono(rule.actions) }, { node: mono(rule.target) }, rule.condition]),
      ),
    ),
  );
  return box;
}

// 이 머신의 오늘. 삭제 가능일과 같은 `YYYY-MM-DD` 모양이다.
function today() {
  return new Date().toLocaleDateString("sv-SE");
}

function lastUseText(user) {
  if (!user.checked_at) return "확인 안 함";
  const seen = user.last_use
    ? `${user.last_use.service} · ${user.last_use.at.slice(0, 16).replace("T", " ")}`
    : "쓰인 적 없음";
  return `${seen}  (${user.checked_at.slice(0, 10)} 확인)`;
}

function keyPane(user) {
  // 묻고 나면 뒷단이 기록하고 목록을 다시 읽게 한다. 이 칸은 기록을 그대로 보여 줄 뿐이다.
  const check = button("확인", {
    onClick: async () => {
      check.disabled = true;
      try {
        await ask("iam_last_used", { at: whereOf(user) });
      } catch {
        check.disabled = false;
      }
    },
  });
  check.className = "quiet";
  const usedLine = document.createElement("span");
  usedLine.className = "cell-actions";
  usedLine.append(span("", lastUseText(user)), check);

  const box = pane(
    "키",
    facts([
      ["키 ID", user.key_id, true],
      ["발급", user.issued_at],
      ["마지막 사용", usedLine],
      ["삭제 가능", user.deletable_from || "모름"],
      ["금고", path(`${user.path}/secret`), true],
    ]),
  );
  box.querySelector(".pane-head").append(copyLines(user, "", ".env 두 줄 복사"));
  return box;
}

function identityPane(user) {
  return pane(
    "IAM",
    facts([
      ["앱", user.app],
      ["환경", user.env],
      [
        "용도",
        purposeField(user.purpose || "", (to) => ask("set_iam_purpose", { at: whereOf(user), to })),
      ],
      ["계정", user.account, true],
      ["만든 계정", user.master, true],
    ]),
  );
}

// 변수 이름에 권한을 넣는다. `.env` 하나에 IAM 이 여럿 들어간다.
function variableOf(user) {
  const perm = user.perm.toUpperCase().replace(/[^A-Z0-9]+/g, "_");
  return `AWS_${perm}_ACCESS_KEY_ID`;
}

// `~/.ssh/config` 의 호스트. 한 번만 읽는다.
let hosts = null;

function knownHosts() {
  const here = { value: "이 맥", detail: "로컬" };
  hosts ??= invoke("ssh_hosts")
    .then((found) => [
      here,
      ...found.map((host) => ({
        value: host.alias,
        detail: [host.address, host.user].filter(Boolean).join(" · "),
      })),
    ])
    .catch(() => [here]);
  return hosts;
}

// `.env` 에 그대로 붙일 두 줄. 시크릿이 금고 밖으로 나가는 유일한 자리다.
function copyLines(user, idVariable, label = "복사") {
  const el = button(label, {
    onClick: async () => {
      try {
        const lines = await ask("iam_env_lines", { at: whereOf(user), idVariable });
        await navigator.clipboard.writeText(lines);
        el.textContent = "복사됨";
        setTimeout(() => (el.textContent = label), 1500);
      } catch {
        /* 터미널 칸에 이미 남았다 */
      }
    },
  });
  return el;
}

function input(id, placeholder, value = "") {
  const el = document.createElement("input");
  el.id = id;
  el.type = "text";
  el.autocomplete = "off";
  el.spellcheck = false;
  el.placeholder = placeholder;
  el.value = value;
  return el;
}

function labelled(text, id, control) {
  const wrap = document.createElement("div");
  wrap.className = "field";
  const label = document.createElement("label");
  label.textContent = text;
  label.htmlFor = id;
  wrap.append(label, control);
  return wrap;
}

// 소비처를 적는 자리. 파일은 건드리지 않는다 — 넣는 일은 사람이 한다.
function addForm(user, onDone) {
  const form = document.createElement("div");
  form.className = "consumer-form";

  const host = input("c-host", "tukapp-prod 또는 이 맥");
  const hostLine = document.createElement("div");
  hostLine.className = "with-chooser";
  hostLine.append(host, chooser(host, { title: "호스트 고르기", load: knownHosts }));

  const file = input("c-path", "~/workspace/back/.env");
  const variable = input("c-var", variableOf(user), variableOf(user));

  const put = button("기록", {
    primary: true,
    onClick: async () => {
      const typed = host.value.trim();
      if (!typed || !file.value.trim()) return;
      put.disabled = true;
      try {
        await ask("add_iam_consumer", {
          at: whereOf(user),
          place: {
            host: typed === "이 맥" ? LOCAL : typed,
            file: file.value.trim(),
            id_variable: variable.value.trim(),
          },
        });
        onDone();
      } catch {
        put.disabled = false;
      }
    },
  });

  const grid = document.createElement("div");
  grid.className = "consumer-grid";
  grid.append(
    labelled("호스트", "c-host", hostLine),
    labelled("파일", "c-path", file),
    labelled("변수", "c-var", variable),
  );

  const actions = document.createElement("div");
  actions.className = "row-actions";
  actions.append(button("취소", { onClick: onDone }), put);

  form.append(grid, actions);
  return form;
}

function consumersPane(user) {
  const box = pane(`소비처 · ${user.consumers.length}`);
  const slot = document.createElement("div");
  slot.className = "consumer-slot";

  const add = button("＋ 추가", {
    onClick: () => {
      add.disabled = true;
      slot.replaceChildren(
        addForm(user, () => {
          slot.replaceChildren();
          add.disabled = false;
        }),
      );
      slot.querySelector("input")?.focus();
    },
  });
  box.querySelector(".pane-head").append(add);

  if (user.consumers.length) {
    box.append(
      table(
        [
          { label: "호스트", width: "14%" },
          { label: "파일", width: "40%" },
          { label: "변수", width: "28%" },
          { label: "", width: "18%" },
        ],
        user.consumers.map((consumer) => {
          const out = armed("빼기", "기록에서 빼기", () =>
            ask("remove_iam_consumer", {
              at: whereOf(user),
              place: {
                host: consumer.host,
                file: consumer.file,
                id_variable: consumer.id_variable,
              },
            }).catch(() => {}),
          );
          const cell = document.createElement("div");
          cell.className = "cell-actions";
          cell.append(copyLines(user, consumer.id_variable), out);
          return row([
            hostLabel(consumer.host),
            { node: mono(consumer.file) },
            { node: mono(consumer.id_variable) },
            { node: cell },
          ]);
        }),
      ),
    );
  } else {
    box.append(span("pane-note", "없습니다."));
  }
  box.append(slot);
  return box;
}

export function renderIam(mount, user) {
  const body = document.createElement("div");
  body.className = "detail-body";
  body.append(rulesPane(user), side(keyPane(user), identityPane(user)), consumersPane(user));

  // 삭제 가능일은 하한이다. 그 전이면 묻지 않고 잠근다. 그 뒤에 누르면 뒷단이 AWS 에
  // 다시 묻고, 그사이 쓰였으면 막으면서 날짜를 미룬다.
  const early = user.deletable_from && today() < user.deletable_from;
  const remove = early
    ? button(`${user.deletable_from} 부터 삭제 가능`, {})
    : armed(
        "삭제",
        "AWS 에서도 지우고 삭제",
        () =>
          ask("remove_iam", { at: whereOf(user) })
            .then(() => select(null))
            .catch(() => {}),
        { danger: true },
      );
  if (early) {
    remove.disabled = true;
    remove.title = "키가 30일 넘게 쓰이지 않아야 지울 수 있습니다";
  }

  mount.replaceChildren(
    back("IAM"),
    head(user.name, `${user.app} · ${user.env} · ${user.service}`, {
      buttons: [remove],
    }),
    body,
  );
}
