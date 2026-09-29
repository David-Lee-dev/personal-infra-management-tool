// 서버 상세 — 계정(접속 · 관리), 쓰는 곳, 정보.

import { button, span } from "../dom.js";
import { modal } from "../modal.js";
import { openAccount } from "./account.js";
import { connect, copyCommand } from "./connect.js";
import { KIND_LABEL, field, segmented, textInput } from "./form.js";
import { openEdit } from "./register.js";
import { invoke } from "../ipc.js";

const ROLE_LABEL = { admin: "관리자", user: "사용자" };
const STATE_LABEL = {
  verified: "확인됨",
  unverified: "확인 전",
  installed: "확인 안 됨",
  local: "서버에 없음",
};
const ORIGIN_NOTE = {
  created: "이 도구가 계정과 키를 만들었습니다",
  installed: "원래 있던 계정에 이 도구가 키를 심었습니다",
  registered: "원래 있던 계정을 기록만 했습니다",
};

/// 두 번 눌러야 실행되는 버튼. 첫 누름에 무엇이 일어나는지 적는다.
function twice(label, armedLabel, run) {
  const el = button(label);
  el.classList.add("danger");
  let armed = false;
  const disarm = () => {
    armed = false;
    el.textContent = label;
    el.classList.remove("armed");
  };
  el.addEventListener("click", () => {
    if (!armed) {
      armed = true;
      el.textContent = armedLabel;
      el.classList.add("armed");
      setTimeout(disarm, 4000);
      return;
    }
    disarm();
    run().catch(() => {});
  });
  return el;
}

function block(title, { note, tools = [] } = {}, ...children) {
  const box = document.createElement("section");
  box.className = "kd-block";
  const head = span("kd-block-head", "");
  head.append(span("kd-block-title", title));
  if (note) head.append(span("kd-block-note", note));
  const right = span("kd-block-tools", "");
  right.append(...tools);
  head.append(right);
  box.append(head, ...children);
  return box;
}

/// 역할 · 용도 고치기. 기록만 바꾼다 — 서버의 권한은 그대로다.
function editAccount(server, account) {
  modal(`계정 편집 · ${server.name}/${account.login}`, (close) => {
    let role = account.role;
    const roles = segmented([["user", "사용자"], ["admin", "관리자 (sudo)"]], role, (r) => (role = r));
    const purpose = textInput(account.purpose, { mono: false });
    const problem = span("problem", "");
    problem.hidden = true;
    const save = button("저장", {
      primary: true,
      onClick: async () => {
        save.disabled = true;
        try {
          await invoke("edit_server_account", { id: server.id, login: account.login, role, purpose: purpose.value });
          close();
        } catch (err) {
          problem.textContent = String(err);
          problem.hidden = false;
          save.disabled = false;
        }
      },
    });
    const actions = document.createElement("div");
    actions.className = "modal-actions";
    actions.append(problem, button("취소", { onClick: close }), save);
    return [
      span("pane-note", "기록만 고칩니다. 서버에서 이 계정의 권한은 바뀌지 않습니다."),
      field("역할", roles.node),
      field("용도", purpose),
      actions,
    ];
  }, { size: "sm" });
}

function accountRow(server, account) {
  const row = document.createElement("div");
  row.className = "sv-account-row";

  const who = span("sv-who", "");
  who.append(span("mono strong", account.login), span(`chip${account.role === "admin" ? " edge" : ""}`, ROLE_LABEL[account.role]));
  if (account.admin_access) who.append(span("chip ok", "관리 접속"));

  const what = span("sv-what", "");
  what.append(span("", account.purpose || "—"), span("muted small", account.key_label));

  const stateText = STATE_LABEL[account.state] ?? account.state;
  const state = span(`sv-state ${account.state === "verified" ? "ok" : "warn"}`, account.verified_at && account.state === "verified" ? `${stateText} ${account.verified_at.slice(5)}` : stateText);
  state.title = ORIGIN_NOTE[account.origin] ?? "";

  const go = button("접속", {
    primary: true,
    onClick: async () => {
      go.disabled = true;
      await connect(server.id, account.login).catch(() => {});
      go.disabled = false;
    },
  });

  // 드물게 쓰는 일은 접어 둔다.
  const more = document.createElement("details");
  more.className = "sv-more";
  const summary = document.createElement("summary");
  summary.textContent = "⋯";
  summary.setAttribute("aria-label", `${account.login} 계정 작업`);
  const menu = span("sv-more-menu", "");
  const copy = button("ssh 명령 복사", {
    onClick: async () => {
      const copied = await copyCommand(server.id, account.login).catch(() => null);
      if (copied !== null) copy.textContent = copied ? "복사됨" : "작업 로그에 적었습니다";
    },
  });
  menu.append(
    copy,
    button("접속 확인", { onClick: () => invoke("check_server_account", { id: server.id, login: account.login }).catch(() => {}) }),
    button("역할 · 용도 편집", { onClick: () => editAccount(server, account) }),
  );
  const used = account.used_by.length ? `쓰는 환경이 있습니다: ${account.used_by.join(", ")}` : "";
  if (account.origin === "registered") {
    const forget = twice("기록에서 빼기", "서버의 계정은 그대로 — 기록만 빼기", () =>
      invoke("forget_server_account", { id: server.id, login: account.login }),
    );
    forget.disabled = Boolean(used);
    forget.title = used;
    menu.append(forget);
  } else {
    menu.append(button("SSH 키 다시 등록", { onClick: () => invoke("reinstall_server_account", { id: server.id, login: account.login }).catch(() => {}) }));
    const remove = twice(
      "서버에서 제거",
      account.origin === "created" ? "서버의 계정까지 지우기" : "키와 권한만 걷기",
      () => invoke("remove_server_account", { id: server.id, login: account.login }),
    );
    remove.disabled = Boolean(used) || account.admin_access;
    remove.title = used || (account.admin_access ? "관리 접속 계정은 서버에서 제거할 수 없습니다." : "");
    menu.append(remove);
  }
  more.append(summary, menu);

  const tools = span("sv-tools", "");
  tools.append(go, more);
  row.append(who, what, state, tools);
  return row;
}

function usesBlock(server) {
  if (!server.uses.length) {
    return block("쓰는 곳", {}, span("kd-empty", "이 서버에 연결된 프로젝트 환경이 없습니다."));
  }
  const list = span("kd-uses", "");
  for (const u of server.uses) {
    const row = span("kd-use", "");
    const chip = span("use-chip", "");
    chip.append(span("use-project", u.project), span("use-env", u.environment));
    row.append(chip, span("kd-use-where mono", `${u.login} · ${u.path} · ${u.branch || "브랜치 없음"}`));
    list.append(row);
  }
  return block("쓰는 곳", { note: "이 서버의 계정으로 연결된 프로젝트 환경" }, list);
}

function factsBlock(server) {
  const list = document.createElement("dl");
  list.className = "kd-slots";
  const rows = [
    ["주소", server.address],
    ["포트", String(server.port)],
    ["종류", KIND_LABEL[server.kind] ?? server.kind],
  ];
  if (server.aws) {
    rows.push(["AWS 계정", server.aws.account], ["리전", server.aws.region], ["인스턴스", server.aws.instance || "—"]);
  }
  rows.push(
    ["관리 접속", server.admin || "지정 안 함"],
    ["공용 디렉터리", `${server.workspace} · 그룹 ${server.workspace_group}`],
    ["메모", server.note || "—"],
    ["등록한 시각", server.registered_at],
  );
  for (const [label, value] of rows) {
    const dt = document.createElement("dt");
    dt.textContent = label;
    const dd = document.createElement("dd");
    dd.textContent = value;
    list.append(dt, dd);
  }
  return block("정보", {}, list);
}

export function renderDetail(mount, server, { groups, onBack, onGone }) {
  const back = document.createElement("button");
  back.type = "button";
  back.className = "back";
  back.textContent = "‹ 서버";
  back.addEventListener("click", onBack);

  const head = document.createElement("div");
  head.className = "detail-head";
  const wrap = span("detail-title-wrap", "");
  const line = span("detail-title-line", "");
  const title = document.createElement("h2");
  title.className = "repo-title mono";
  title.textContent = server.name;
  line.append(title, span(`chip sv-kind ${server.kind === "other" ? "other" : "aws"}`, KIND_LABEL[server.kind] ?? server.kind));
  if (server.group) line.append(span("chip", server.group));
  const address = server.port === 22 ? server.address : `${server.address}:${server.port}`;
  const meta = [address, server.aws?.region, server.aws?.instance].filter(Boolean).join(" · ");
  wrap.append(line, span("detail-sub mono", meta));
  const inUse = server.uses.map((u) => `${u.project}/${u.environment}`);
  const unregister = twice(
    "등록 해제",
    server.accounts.some((a) => a.origin !== "registered")
      ? "이 도구가 심은 계정은 서버에 남습니다 — 기록만 보관"
      : "기록을 보관소로 옮기기",
    () => invoke("unregister_server", { id: server.id }).then(onGone),
  );
  unregister.disabled = inUse.length > 0;
  unregister.title = inUse.length ? `쓰는 환경이 있어 해제할 수 없습니다: ${inUse.join(", ")}` : "";
  const actions = span("head-actions", "");
  actions.append(button("편집", { onClick: () => openEdit(server, { groups }) }), unregister);
  head.append(wrap, actions);

  const accounts = block(
    "계정",
    {
      note: `${server.accounts.length}개 · [접속]은 Ghostty 새 창에서 그 계정으로 들어갑니다`,
      tools: [button("＋ 계정", { onClick: () => openAccount(server) })],
    },
    ...server.accounts.map((a) => accountRow(server, a)),
  );
  if (!server.accounts.length) accounts.append(span("kd-empty", "계정이 없습니다. ＋ 계정으로 더하세요."));

  const body = document.createElement("div");
  body.className = "kd-body sv-detail";
  body.append(accounts, usesBlock(server), factsBlock(server));
  mount.replaceChildren(back, head, body);
}
