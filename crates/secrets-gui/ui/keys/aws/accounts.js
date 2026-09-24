// pem 아래의 인스턴스 접속 계정.
//
// pem 으로 서버에 들어가 계정을 만들고 키를 심는다. 이 도구가 남의 서버를 고치는
// 유일한 자리라, 무엇이 돌았는지는 터미널 칸에 그대로 흐른다.

import { button, facts, pane, span } from "../../dom.js";
import { termWrite } from "../../terminal.js";

const { invoke } = window.__TAURI__.core;

const ROLE_LABEL = { admin: "관리자", user: "사용자" };
const STATE_LABEL = {
  local: "서버에 없음",
  installed: "확인 안 됨",
  verified: "확인됨",
};

function ask(command, args) {
  return invoke(command, args).catch((err) => {
    termWrite("err", String(err));
    throw err;
  });
}

/// 화면이 들고 있는 값에서 서버 자리를 만든다.
export function whereOf(key, account, extra = {}) {
  return {
    aws_account: key.account,
    machine: key.machine,
    region: key.region,
    keypair: key.name,
    instance: extra.instance ?? "",
    account,
    via: extra.via ?? "ubuntu",
    address: extra.address ?? "",
    workspace: extra.workspace ?? "/srv",
    group: extra.group ?? "workspace",
  };
}

function stateChip(account) {
  const tone = account.state === "verified" ? "ok" : "warn";
  return span(`chip ${tone}`, STATE_LABEL[account.state] ?? account.state);
}

// 계정 한 줄. 무엇이고 어디까지 갔는지가 한눈에 들어야 한다.
function row(account, onOpen) {
  const line = document.createElement("button");
  line.type = "button";
  line.className = "host-row";

  line.append(span("host-name", account.account));
  line.append(span("host-role", ROLE_LABEL[account.role] ?? account.role));
  if (account.purpose) line.append(span("host-purpose", account.purpose));
  line.append(span("host-gap", ""));
  line.append(stateChip(account));
  line.addEventListener("click", () => onOpen(account));
  return line;
}

/// pem 상세 아래에 붙는 칸.
export function accountsPane(key, accounts, { onCreate, onOpen }) {
  const box = pane("이 pem 키의 접속 계정");

  const head = box.querySelector(".pane-head");
  head.append(button("＋ 계정 만들기", { primary: true, onClick: onCreate }));

  if (!accounts.length) {
    box.append(
      span(
        "pane-note",
        "아직 계정이 없습니다. 계정을 생성하면 pem 키로 서버에 접속해 SSH 키를 등록합니다.",
      ),
    );
    return box;
  }

  // 인스턴스로 묶는다. 계정은 인스턴스의 자식이다.
  const byInstance = new Map();
  for (const account of accounts) {
    if (!byInstance.has(account.instance)) byInstance.set(account.instance, []);
    byInstance.get(account.instance).push(account);
  }

  for (const [instance, mine] of byInstance) {
    const first = mine[0];
    box.append(
      span("host-instance", `${first.instance_name || instance}  ·  ${first.address}`),
    );
    for (const account of mine) box.append(row(account, onOpen));
  }
  return box;
}

/// 계정 하나의 상세.
export function accountDetail(key, account, { onBack, onChanged }) {
  const at = whereOf(key, account.account, account);

  const box = pane(
    "계정",
    facts([
      ["계정", account.account, true],
      ["역할", ROLE_LABEL[account.role] ?? account.role],
      ["용도", account.purpose || "—"],
      ["상태", STATE_LABEL[account.state] ?? account.state],
      ["확인한 시각", account.verified_at?.slice(0, 19).replace("T", " ") ?? "아직"],
      ["인스턴스", `${account.instance_name || account.instance}  ·  ${account.address}`, true],
      ["들어갈 때", `${account.via} → ${account.account}`, true],
      ["공용 자리", `${account.workspace}  (그룹 ${account.group})`, true],
      ["지문", account.fingerprint, true],
      ["이 도구로 만든 계정", account.ours ? "예" : "아니요 — 서버에서 제거해도 계정은 유지됩니다"],
    ]),
  );

  const actions = document.createElement("div");
  actions.className = "row-actions";
  actions.append(
    button("연결", {
      primary: true,
      onClick: () =>
        invoke("connect_instance_account", { at }).catch((err) => termWrite("err", String(err))),
    }),
    button("SSH 키 다시 등록", {
      onClick: () =>
        ask("reinstall_instance_account", { at, role: account.role })
          .then(onChanged)
          .catch(() => {}),
    }),
    removeButton(at, account, onChanged),
  );
  box.append(actions);

  const back = document.createElement("button");
  back.type = "button";
  back.className = "back";
  back.textContent = "‹ pem 키";
  back.addEventListener("click", onBack);

  const wrap = document.createElement("div");
  wrap.className = "detail-body";
  wrap.append(box);
  return [back, wrap];
}

// 되돌릴 수 있게 실물은 보관한다. 서버 쪽은 먼저 걷는다.
function removeButton(at, account, onChanged) {
  const el = document.createElement("button");
  el.type = "button";
  el.className = "danger";
  el.textContent = "서버에서 제거";

  let armed = false;
  const disarm = () => {
    armed = false;
    el.textContent = "서버에서 제거";
    el.classList.remove("armed");
  };

  el.addEventListener("click", () => {
    if (!armed) {
      armed = true;
      el.textContent = account.ours ? "서버 계정까지 지우기" : "키와 권한만 제거";
      el.classList.add("armed");
      setTimeout(disarm, 4000);
      return;
    }
    disarm();
    ask("remove_instance_account", { at }).then(onChanged).catch(() => {});
  });
  return el;
}
