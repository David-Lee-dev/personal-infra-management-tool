// 서버에 계정 더하기 — 두 갈래.
//
// - 서버에 새로 만들기: 관리 접속으로 들어가 계정을 만들고 이 도구가 만든 키를 심는다. 만들기 전에
//   **점검**한다. `setfacl` 이 없으면 공용 자리 쓰기 공유가 umask 에 좌우되는데, 그건 몇 주 뒤에야
//   드러난다. 반만 도는 계정을 만드는 것보다 안 만드는 게 낫다.
// - 원래 있는 계정 등록: 로그인 · 역할 · 키만 적는다. 서버에 쓰지 않는다.

import { button, span } from "../dom.js";
import { modal } from "../modal.js";
import { termWrite } from "../terminal.js";
import { field, heldPems, keyChooser, segmented, textInput } from "./form.js";

const { invoke } = window.__TAURI__.core;

const ROLES = [
  { value: "user", label: "사용자" },
  { value: "admin", label: "관리자" },
];

// 계정 한 줄 — 이름 · 역할 · 용도. 결과 칸은 만들기가 실패했을 때만 채운다.
function seatRow(onRemove, preset = { account: "", role: "user", purpose: "" }) {
  const row = document.createElement("div");
  row.className = "seat-row";
  const account = textInput(preset.account, { placeholder: "새 로그인 이름" });
  account.setAttribute("aria-label", "계정 이름");
  const role = document.createElement("select");
  role.setAttribute("aria-label", "역할");
  for (const option of ROLES) {
    const item = document.createElement("option");
    item.value = option.value;
    item.textContent = option.label;
    role.append(item);
  }
  role.value = preset.role;
  const purpose = textInput(preset.purpose, { mono: false, placeholder: "용도" });
  purpose.setAttribute("aria-label", "용도");
  const remove = button("제거", { onClick: () => onRemove(row) });
  const result = span("seat-result", "");
  result.hidden = true;
  row.append(account, role, purpose, remove, result);
  return {
    row,
    value: () => ({ account: account.value.trim(), role: role.value, purpose: purpose.value.trim() }),
    fail(text) {
      result.textContent = text;
      result.hidden = !text;
    },
  };
}

function createPane(server, close) {
  if (!server.admin) {
    return [
      span("notice warn", "이 서버에는 관리 접속이 지정되지 않아 새 계정을 만들 수 없습니다."),
      span("pane-note", "sudo 가 있는 계정을 [원래 있는 계정 등록]으로 기록하고, 서버 [편집]에서 관리 접속으로 지정하세요."),
    ];
  }
  const taken = new Set(server.accounts.map((a) => a.login));
  const seats = [];
  const rows = document.createElement("div");
  rows.className = "seat-rows";
  const chips = document.createElement("div");
  chips.className = "seat-snippets";
  // 이미 넣었거나 이 서버에 이미 있는 계정의 버튼은 흐리게 한다.
  function refreshChips() {
    const listed = new Set(seats.map((s) => s.value().account));
    for (const chip of chips.querySelectorAll("button[data-account]")) {
      chip.disabled = listed.has(chip.dataset.account) || taken.has(chip.dataset.account);
    }
  }
  function addSeat(preset) {
    const seat = seatRow((row) => {
      seats.splice(seats.findIndex((s) => s.row === row), 1);
      row.remove();
      refreshChips();
    }, preset);
    seats.push(seat);
    rows.append(seat.row);
    refreshChips();
  }
  const more = button("＋ 직접 입력", { onClick: () => addSeat() });
  chips.append(more);
  invoke("known_server_accounts")
    .catch(() => [])
    .then((known) => {
      for (const k of known) {
        const chip = button(`＋ ${k.login}`, {
          onClick: () => addSeat({ account: k.login, role: k.admin ? "admin" : "user", purpose: "" }),
        });
        chip.dataset.account = k.login;
        chip.title = taken.has(k.login) ? "이 서버에 이미 있음" : `${k.admin ? "관리자" : "사용자"} · 서버 ${k.servers}대에 있음`;
        chips.insertBefore(chip, more);
      }
      refreshChips();
    });

  const verdict = document.createElement("div");
  verdict.className = "checklist";
  // 항목마다 표시를 단다. 되는 쪽과 안 되는 쪽이 같은 모양이면 훑을 때 구분이 안 된다.
  function show(seen) {
    verdict.replaceChildren();
    for (const [id, label] of [["sudo", "sudo"], ["useradd", "useradd"], ["visudo", "visudo"], ["acl", "setfacl"]]) {
      const box = span(seen[id] ? "check on" : "check off", "");
      box.append(span("check-mark", seen[id] ? "✓" : "✗"), span("check-name", label));
      verdict.append(box);
    }
  }
  const say = (text, tone) => verdict.replaceChildren(span(`resolved ${tone}`.trim(), text));

  const create = button("만들기", { primary: true });
  create.disabled = true;
  const check = button("점검", {
    onClick: async () => {
      check.disabled = true;
      check.textContent = "서버 확인 중…";
      try {
        const seen = await invoke("inspect_server", { id: server.id });
        show(seen);
        prepare.hidden = seen.ok || !seen.missing.includes("setfacl");
        create.disabled = !seen.ok;
      } catch (err) {
        say(String(err), "bad");
        create.disabled = true;
      } finally {
        check.disabled = false;
        check.textContent = "다시 점검";
      }
    },
  });
  // 패키지를 까는 건 이 도구가 서버 구성에 손대는 유일한 자리다. 누를 때만 돈다.
  const prepare = button("acl 설치", {
    onClick: async () => {
      prepare.disabled = true;
      try {
        const seen = await invoke("prepare_server", { id: server.id });
        show(seen);
        prepare.hidden = seen.ok;
        create.disabled = !seen.ok;
      } catch (err) {
        termWrite("err", String(err));
      } finally {
        prepare.disabled = false;
      }
    },
  });
  prepare.hidden = true;

  create.addEventListener("click", async () => {
    const wanted = seats.filter((seat) => seat.value().account);
    if (!wanted.length) {
      say("생성할 계정 이름을 입력하세요", "bad");
      return;
    }
    create.disabled = true;
    for (const seat of seats) seat.fail("");
    try {
      const outcomes = await invoke("create_server_accounts", { id: server.id, accounts: wanted.map((s) => s.value()) });
      const failed = outcomes.filter((o) => o.error);
      for (const o of failed) termWrite("err", `${o.account}: ${o.error}`);
      if (!failed.length) return close();
      for (const o of failed) wanted.find((s) => s.value().account === o.account)?.fail(o.error);
      create.disabled = false;
    } catch (err) {
      termWrite("err", String(err));
      create.disabled = false;
    }
  });

  const probe = span("with-chooser", "");
  probe.append(check, prepare);
  const actions = document.createElement("div");
  actions.className = "modal-actions";
  actions.append(button("취소", { onClick: close }), create);

  return [
    span(
      "sv-admin-line",
      `관리 접속 ${server.admin}으로 들어가 계정을 만들고, 이 도구가 만든 키를 심은 뒤 그 키로 다시 들어가 확인합니다. 공용 자리: ${server.workspace} · 그룹 ${server.workspace_group}.`,
    ),
    field("서버 점검", probe),
    verdict,
    field("만들 계정", chips),
    rows,
    span(
      "pane-note",
      "위 버튼은 다른 서버에서 쓰는 계정 이름입니다. 이 서버에 이미 있는 이름은 만들 수 없으니 사람이나 프로젝트별로 다른 이름(예: deploy-garden)을 입력하세요. 사용자는 자기 홈 디렉터리와 공용 작업 디렉터리에만 접근하고 sudo가 없습니다. 관리자는 sudo가 있습니다.",
    ),
    actions,
  ];
}

function registerPane(server, pems, close) {
  const login = textInput("");
  let role = "user";
  const roles = segmented([["user", "사용자"], ["admin", "관리자 (sudo)"]], role, (r) => (role = r));
  const purpose = textInput("", { mono: false, placeholder: "선택" });
  const usable = server.kind === "other" || !server.aws
    ? []
    : pems.filter((p) => p.machine === server.kind && p.account === server.aws.account && p.region === server.aws.region);
  const key = keyChooser({ pems: usable.map((p) => p.name), initial: "file" });

  const problem = span("problem", "");
  problem.hidden = true;
  const save = button("등록", {
    primary: true,
    onClick: async () => {
      problem.hidden = true;
      save.disabled = true;
      try {
        const name = login.value.trim();
        await invoke("add_server_account", {
          id: server.id,
          form: { login: name, role, purpose: purpose.value.trim(), key: key.value() },
        });
        close();
        invoke("check_server_account", { id: server.id, login: name }).catch((err) =>
          termWrite("err", `${server.name}/${name} 접속 확인: ${err}`),
        );
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
    span("pane-note", "서버에 쓰지 않습니다. 등록한 뒤 그 계정으로 한 번 들어가 봅니다."),
    field("로그인", login),
    field("역할", roles.node, "기록입니다. 서버의 권한은 바꾸지 않습니다."),
    field("용도", purpose),
    field("키", key.node),
    actions,
  ];
}

/// 계정 추가 창.
export function openAccount(server) {
  modal(`계정 추가 · ${server.name}`, (close) => {
    const holder = document.createElement("div");
    holder.className = "git-form";
    const pane = document.createElement("div");
    pane.className = "git-form";
    let pems = [];
    const tabs = segmented(
      [["create", "서버에 새로 만들기"], ["register", "원래 있는 계정 등록"]],
      server.admin ? "create" : "register",
      (tab) => draw(tab),
    );
    function draw(tab) {
      pane.replaceChildren(...(tab === "create" ? createPane(server, close) : registerPane(server, pems, close)));
    }
    holder.append(tabs.node, pane);
    heldPems().then((found) => {
      pems = found;
      draw(tabs.value());
    });
    return [holder];
  }, { size: "lg" });
}
