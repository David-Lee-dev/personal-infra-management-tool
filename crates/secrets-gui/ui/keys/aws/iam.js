// IAM — 목록 칸과 상세.
//
// IAM 하나는 권한 하나다. 무엇을 할 수 있는지가 가장 먼저 보이고, 그다음이 키,
// 마지막이 그 키를 넣었다고 적은 곳이다. `.env` 에 넣고 빼는 일은 사람이 하고,
// 이 화면은 기록과 붙여 넣을 두 줄만 준다.
//
// 키를 바꾸는 버튼은 없다. 새 IAM 을 만들고 옛 것은 치운다 — 옛 것은 키가 한 달
// 넘게 쓰이지 않아야 지워진다.
//
// 금고 밖에서 만든 IAM 도 기록으로 들일 수 있다. 들인 IAM 은 시크릿이 없어 복사할 것이
// 없다. 치울 것은 "정리 대상" 으로 분류해 한 묶음으로 모아 본다.

import { button, facts, pane, path, span } from "../../dom.js";
import { chooser } from "../../combo.js";
import { ask, back, expiryField, head, purposeField, side } from "../parts.js";
import { listFilter, select } from "../state.js";
import { iamUsers } from "./state.js";
import { row, table } from "../table.js";
import { chip, expiryChips, group, line, matches, nothing, toolbar } from "../kit.js";
import { projectChip, unplaced, useChips, useOfPlace, usesOf } from "../usage.js";

const { invoke } = window.__TAURI__.core;

const ENV_ORDER = ["prod", "dev", "local"];
const LOCAL = "local";

function hostLabel(host) {
  return host === LOCAL ? "로컬" : host;
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

const ADOPTED = "adopted";

function byEnvThenName(a, b) {
  return ENV_ORDER.indexOf(a.env) - ENV_ORDER.indexOf(b.env) || a.name.localeCompare(b.name);
}

function iamLine(user) {
  const uses = usesOf("iam:" + user.ref);
  const chips = [];
  if (user.env) chips.push(chip(user.env));
  chips.push(chip(user.service || "?"));
  if (user.cleanup) chips.push(chip("폐기 예정", "warn"));
  if (user.origin === ADOPTED) chips.push(chip("들인 IAM"));
  chips.push(...expiryChips(user.expiry));
  return line({
    title: user.name,
    sub: user.purpose || user.scope,
    chips,
    uses: useChips(uses, { extra: unplaced(user.consumers, uses) }),
    tone: user.cleanup ? "warn" : "",
    onClick: () => select({ kind: "iam", ref: user.ref }),
  });
}

const FILTERS = {
  all: () => true,
  prod: (u) => u.env === "prod",
  dev: (u) => u.env === "dev",
  local: (u) => u.env === "local",
  unused: (u) => !u.consumers.length,
  cleanup: (u) => Boolean(u.cleanup),
};

// 정리 대상이 맨 위다. 나머지는 이름이 `<앱>-<환경>-<권한>-iam` 이라 앱이 곧 묶음이고,
// 규칙 밖 이름인 들인 IAM 은 따로 모은다.
export function renderIamList(mount) {
  const users = iamUsers();
  const bar = toolbar({
    placeholder: "이름 · 앱 · 용도로 찾기",
    filters: [
      { id: "all", label: "전체", count: users.length },
      { id: "prod", label: "prod", count: users.filter(FILTERS.prod).length },
      { id: "dev", label: "dev", count: users.filter(FILTERS.dev).length },
      { id: "local", label: "local", count: users.filter(FILTERS.local).length },
      { id: "unused", label: "기록된 사용 위치 없음", count: users.filter(FILTERS.unused).length, tone: "warn" },
      { id: "cleanup", label: "폐기 예정", count: users.filter(FILTERS.cleanup).length, tone: "warn" },
    ],
  });
  const shown = users
    .filter(FILTERS[listFilter()] ?? FILTERS.all)
    .filter((u) => matches(u.name, u.app, u.purpose, u.service, u.scope));

  const groups = [];
  const marked = shown.filter((user) => user.cleanup);
  if (marked.length) groups.push(["폐기 예정", marked, "새 IAM으로 바꾸고 30일 동안 쓰이지 않으면 지웁니다"]);
  const rest = shown.filter((user) => !user.cleanup);
  const issued = rest.filter((user) => user.origin !== ADOPTED);
  for (const app of [...new Set(issued.map((user) => user.app))].sort()) {
    groups.push([app, issued.filter((user) => user.app === app)]);
  }
  const adopted = rest.filter((user) => user.origin === ADOPTED);
  if (adopted.length) groups.push(["들인 IAM · 명명 규칙과 다름", adopted]);

  const parts = [bar];
  if (!shown.length) parts.push(nothing(users.length ? "찾는 IAM이 없습니다." : "IAM이 없습니다. ＋ IAM 만들기로 시작하세요."));
  for (const [label, members, note] of groups) {
    parts.push(group(label, [...members].sort(byEnvThenName).map(iamLine), { note }));
  }
  mount.replaceChildren(...parts);
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
    : "사용 기록 없음";
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

  const adopted = user.origin === ADOPTED;
  const box = pane(
    "키",
    facts([
      ["키 ID", user.key_id, true],
      ["발급", user.issued_at],
      ["마지막 사용", usedLine],
      ["삭제 가능 예정일", user.deletable_from || "모름"],
      adopted
        ? ["시크릿", "없음 — 이 도구에서 생성하지 않은 IAM"]
        : ["저장 위치", path(`${user.path}/secret`), true],
    ]),
  );
  return box;
}

function identityPane(user) {
  return pane(
    "IAM",
    facts([
      ["출처", user.origin === ADOPTED ? "등록됨 — 명명 규칙과 다름" : "이 도구로 발급"],
      ["앱", user.app || "—"],
      ["환경", user.env || "—"],
      [
        "용도",
        purposeField(user.purpose || "", (to) => ask("set_iam_purpose", { at: whereOf(user), to })),
      ],
      ["만료", expiryField(user.expiry, (to) => ask("set_iam_expires", { at: whereOf(user), to }))],
      ["계정", user.account, true],
      ["만든 계정", user.master, true],
    ]),
  );
}

// 변수 이름에 권한을 넣는다. `.env` 하나에 IAM 이 여럿 들어간다. 들인 IAM 은 권한
// 조각이 없어 SDK 기본 이름을 권한다.
function variableOf(user) {
  if (!user.perm) return "AWS_ACCESS_KEY_ID";
  const perm = user.perm.toUpperCase().replace(/[^A-Z0-9]+/g, "_");
  return `AWS_${perm}_ACCESS_KEY_ID`;
}

// `~/.ssh/config` 의 호스트. 한 번만 읽는다.
let hosts = null;

function knownHosts() {
  const here = { value: "로컬", detail: "로컬" };
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

  const host = input("c-host", "tukapp-prod 또는 로컬");
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
            host: typed === "로컬" ? LOCAL : typed,
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

/// 프로젝트의 파일이면 프로젝트 안의 경로로 짧게, 전체 경로는 툴팁으로.
function fileCell(consumer, use) {
  const el = mono(use ? use.file : consumer.file);
  el.title = consumer.file;
  return el;
}

function consumersPane(user) {
  const uses = usesOf("iam:" + user.ref);
  const box = pane(`쓰는 곳 · ${user.consumers.length}`);
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
          { label: "프로젝트", width: "20%" },
          { label: "호스트", width: "12%" },
          { label: "파일", width: "32%" },
          { label: "변수", width: "20%" },
          { label: "", width: "16%" },
        ],
        user.consumers.map((consumer) => {
          const use = useOfPlace(uses, consumer);
          const out = armed("제거", "사용 위치 기록에서 제거", () =>
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
          if (user.origin !== ADOPTED) cell.append(copyLines(user, consumer.id_variable));
          cell.append(out);
          return row([
            { node: use ? projectChip(use) : span("muted small", "—") },
            hostLabel(consumer.host),
            { node: fileCell(consumer, use) },
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

// 정리 대상 분류. 분류는 기록일 뿐 AWS 는 바뀌지 않는다. 지우는 기준은 여전히 마지막 사용이다.
function cleanupPane(user) {
  const box = pane("폐기");
  const mark = user.cleanup;
  if (mark) {
    const off = button("지정 해제", {
      onClick: async () => {
        off.disabled = true;
        await ask("unmark_iam_cleanup", { at: whereOf(user) }).catch(() => {
          off.disabled = false;
        });
      },
    });
    off.className = "quiet";
    box.querySelector(".pane-head").append(off);
    box.append(
      facts([
        ["지정", `폐기 예정 · ${mark.marked_at.slice(0, 10)}`],
        [
          "사유",
          purposeField(mark.reason, (reason) =>
            ask("mark_iam_cleanup", { at: whereOf(user), reason }),
          ),
        ],
      ]),
    );
    return box;
  }

  const reason = input("cleanup-reason", "사유 — 예: tuk-api-prod-s3-iam-20260924로 교체");
  const on = button("폐기 대상으로 지정", {
    onClick: async () => {
      on.disabled = true;
      await ask("mark_iam_cleanup", { at: whereOf(user), reason: reason.value.trim() }).catch(() => {
        on.disabled = false;
      });
    },
  });
  const line = document.createElement("div");
  line.className = "cell-actions";
  line.append(reason, on);
  box.append(line);
  return box;
}

export function renderIam(mount, user) {
  const body = document.createElement("div");
  body.className = "detail-body";
  // 쓰는 곳이 먼저다. 무엇을 할 수 있는지(권한)와 키 · 기록은 그다음.
  body.append(
    consumersPane(user),
    rulesPane(user),
    side(keyPane(user), identityPane(user)),
    cleanupPane(user),
  );

  // 삭제 가능일은 하한이다. 그 전이면 묻지 않고 잠근다. 그 뒤에 누르면 뒷단이 AWS 에
  // 다시 묻고, 그사이 쓰였으면 막으면서 날짜를 미룬다.
  const early = user.deletable_from && today() < user.deletable_from;
  const remove = early
    ? button(`${user.deletable_from}부터 삭제 가능`, {})
    : armed(
        "삭제",
        "AWS에서도 삭제",
        () =>
          ask("remove_iam", { at: whereOf(user) })
            .then(() => select(null))
            .catch(() => {}),
        { danger: true },
      );
  if (early) {
    remove.disabled = true;
    remove.title = "키를 삭제하려면 마지막 사용 후 30일이 지나야 합니다";
  }

  const sub =
    user.origin === ADOPTED
      ? `등록한 IAM · ${user.service}`
      : `${user.app} · ${user.env} · ${user.service}`;
  const badges = user.cleanup ? [span("badge-warn", "폐기 예정")] : [];
  const copy = user.origin === ADOPTED ? null : copyLines(user, "", ".env 두 줄 복사");
  // 들인 IAM 은 정책을 바꾸지 않는다 — 새로 발급해 옮긴다(규칙 9).
  const policy = button("정책 바꾸기", { onClick: () => select({ kind: "iam-policy", ref: user.ref }) });
  if (user.origin === ADOPTED) {
    policy.disabled = true;
    policy.title = "등록한 IAM은 정책을 바꾸지 않습니다. 새 IAM을 발급해 옮기세요(규칙 9).";
  }
  mount.replaceChildren(
    back("AWS IAM"),
    head(user.name, sub, { badges, buttons: [copy, policy, remove].filter(Boolean) }),
    body,
  );
}
