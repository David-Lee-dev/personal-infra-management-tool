// 인스턴스 계정 만들기. 한 인스턴스에 여러 계정을 한 번에 만든다.
//
// 만들기 전에 **점검**한다. 서버에 `setfacl` 이 없으면 공용 자리 쓰기 공유가
// umask 에 좌우되는데, 그건 몇 주 뒤에야 드러난다. 반만 도는 계정을 만드는 것보다
// 안 만드는 게 낫다.

import { button, span } from "../../dom.js";
import { termWrite } from "../../terminal.js";
import { whereOf } from "./accounts.js";

const { invoke } = window.__TAURI__.core;

function field(label, id, placeholder, value = "") {
  const wrap = document.createElement("div");
  wrap.className = "field";

  const el = document.createElement("label");
  el.textContent = label;
  el.htmlFor = id;

  const input = document.createElement("input");
  input.id = id;
  input.type = "text";
  input.autocomplete = "off";
  input.spellcheck = false;
  input.placeholder = placeholder;
  input.value = value;

  wrap.append(el, input);
  return { wrap, input };
}

const ROLES = [
  { value: "user", label: "사용자" },
  { value: "admin", label: "관리자" },
];

/// 다른 서버에 이미 있는 계정을 한 줄씩 넣는 버튼. 무엇을 만들지는 사용자가 정한다.
async function knownAccounts() {
  try {
    return await invoke("known_server_accounts");
  } catch {
    return [];
  }
}

// 계정 한 줄 — 이름 · 역할 · 용도. 결과 칸은 만들기가 실패했을 때만 채운다.
function seatRow(onRemove, preset = { account: "", role: "user", purpose: "" }) {
  const row = document.createElement("div");
  row.className = "seat-row";

  const account = document.createElement("input");
  account.type = "text";
  account.autocomplete = "off";
  account.spellcheck = false;
  account.placeholder = "계정 이름";
  account.value = preset.account;
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

  const purpose = document.createElement("input");
  purpose.type = "text";
  purpose.autocomplete = "off";
  purpose.placeholder = "용도";
  purpose.value = preset.purpose;
  purpose.setAttribute("aria-label", "용도");

  const remove = button("제거", { onClick: () => onRemove(row) });
  const result = span("seat-result", "");
  result.hidden = true;

  row.append(account, role, purpose, remove, result);
  return {
    row,
    value: () => ({
      account: account.value.trim(),
      role: role.value,
      purpose: purpose.value.trim(),
    }),
    fail(text) {
      result.textContent = text;
      result.hidden = !text;
    },
  };
}

export function renderNewAccount(mount, key, { onBack, onChanged }) {
  const instance = field("인스턴스 ID", "h-instance", "i-…");
  const name = field("인스턴스 이름", "h-name", "AWS 콘솔의 Name 태그");
  const address = field("주소", "h-address", "공인 IP 또는 DNS");
  const via = field("접속 계정", "h-via", "ubuntu", "ubuntu");
  const workspace = field("공용 작업 디렉터리", "h-workspace", "/srv", "/srv");
  const group = field("공용 그룹", "h-group", "workspace", "workspace");

  const seats = [];
  const rows = document.createElement("div");
  rows.className = "seat-rows";
  const chips = document.createElement("div");
  chips.className = "seat-snippets";
  // 이미 넣은 규칙 계정의 버튼은 흐리게 한다. 줄을 지우면 다시 살린다.
  function refreshChips() {
    const taken = new Set(seats.map((s) => s.value().account));
    for (const chip of chips.querySelectorAll("button[data-account]")) {
      chip.disabled = taken.has(chip.dataset.account);
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
  knownAccounts().then((known) => {
    for (const k of known) {
      const preset = { account: k.login, role: k.admin ? "admin" : "user", purpose: "" };
      const chip = button(`＋ ${k.login}`, { onClick: () => addSeat(preset) });
      chip.dataset.account = k.login;
      chip.title = `${k.admin ? "관리자" : "사용자"} · 서버 ${k.servers}대에 있음`;
      chips.insertBefore(chip, more);
    }
    refreshChips();
  });

  const seatsField = document.createElement("div");
  seatsField.className = "field";
  seatsField.append(span("field-label", "만들 계정"), chips, rows);

  const note = span(
    "pane-note",
    "위의 버튼은 다른 서버에 이미 있는 계정입니다. 눌러서 같은 이름 · 역할로 넣거나 직접 입력하세요. 사용자는 자신의 홈 디렉터리와 공용 작업 디렉터리에만 접근할 수 있으며 sudo 권한이 없습니다. 관리자는 sudo 권한이 있고 다른 계정의 홈 디렉터리에도 접근할 수 있습니다.",
  );

  const verdict = document.createElement("div");
  verdict.className = "checklist";
  let ready = false;

  // 항목마다 표시를 단다. 되는 쪽과 안 되는 쪽이 같은 모양이면 훑을 때 구분이 안 된다.
  function show(seen) {
    verdict.replaceChildren();
    for (const [id, label] of [
      ["sudo", "sudo"],
      ["useradd", "useradd"],
      ["visudo", "visudo"],
      ["acl", "setfacl"],
    ]) {
      const box = span(seen[id] ? "check on" : "check off", "");
      box.append(span("check-mark", seen[id] ? "✓" : "✗"), span("check-name", label));
      verdict.append(box);
    }
  }

  function say(text, tone) {
    verdict.replaceChildren(span(`resolved ${tone}`.trim(), text));
  }

  function target(login = "probe") {
    return whereOf(key, login, {
      instance: instance.input.value.trim(),
      address: address.input.value.trim(),
      via: via.input.value.trim(),
      workspace: workspace.input.value.trim(),
      group: group.input.value.trim(),
    });
  }

  const check = button("점검", {
    onClick: async () => {
      if (!address.input.value.trim()) {
        say("주소를 입력하세요", "bad");
        return;
      }
      check.disabled = true;
      check.textContent = "서버 확인 중…";
      try {
        const seen = await invoke("inspect_instance", { at: target() });
        ready = seen.ok;
        show(seen);
        prepare.hidden = seen.ok || !seen.missing.includes("setfacl");
        create.disabled = !seen.ok;
      } catch (err) {
        ready = false;
        say(String(err), "bad");
        create.disabled = true;
      } finally {
        check.disabled = false;
        check.textContent = "점검";
      }
    },
  });

  // 패키지를 까는 건 이 도구가 서버 구성에 손대는 유일한 자리다. 누를 때만 돈다.
  const prepare = button("acl 설치", {
    onClick: async () => {
      prepare.disabled = true;
      try {
        const seen = await invoke("prepare_instance", { at: target() });
        ready = seen.ok;
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

  const create = button("만들기", {
    primary: true,
    onClick: async () => {
      const wanted = seats.filter((seat) => seat.value().account);
      if (!wanted.length) {
        say("생성할 계정 이름을 입력하세요", "bad");
        return;
      }
      create.disabled = true;
      for (const seat of seats) seat.fail("");
      try {
        const outcomes = await invoke("create_instance_accounts", {
          at: target(),
          accounts: wanted.map((seat) => seat.value()),
          instanceName: name.input.value.trim(),
        });
        const failed = outcomes.filter((o) => o.error);
        for (const o of failed) termWrite("err", `${o.account}: ${o.error}`);
        // 하나라도 만들어졌으면 목록이 새로 그려지며 이 폼은 사라진다. 실패는 터미널에 남는다.
        if (failed.length < outcomes.length) return onChanged();
        for (const o of failed) {
          wanted.find((seat) => seat.value().account === o.account)?.fail(o.error);
        }
        create.disabled = false;
      } catch (err) {
        termWrite("err", String(err));
        create.disabled = false;
      }
    },
  });
  create.disabled = true;

  // 무엇이든 바뀌면 점검은 무효다. 옛 점검으로 만들면 엉뚱한 서버에 심는다.
  const stale = () => {
    ready = false;
    create.disabled = true;
    verdict.replaceChildren();
    prepare.hidden = true;
  };
  for (const box of [instance, address, via, workspace, group]) {
    box.input.addEventListener("input", stale);
  }

  const form = document.createElement("form");
  form.className = "account-form";
  form.autocomplete = "off";
  form.addEventListener("submit", (event) => event.preventDefault());

  const heading = document.createElement("div");
  heading.className = "detail-head";
  const wrap = document.createElement("div");
  wrap.className = "detail-title-wrap";
  const h2 = document.createElement("h2");
  h2.textContent = "계정 만들기";
  wrap.append(h2);
  heading.append(wrap);

  const probe = document.createElement("div");
  probe.className = "with-chooser";
  probe.append(address.input, check, prepare);
  address.wrap.append(probe);

  const body = document.createElement("div");
  body.className = "detail-body";
  body.append(
    instance.wrap,
    name.wrap,
    address.wrap,
    verdict,
    via.wrap,
    seatsField,
    note,
    workspace.wrap,
    group.wrap,
  );

  const actions = document.createElement("div");
  actions.className = "detail-actions";
  actions.append(button("취소", { onClick: onBack }), create);

  const back = document.createElement("button");
  back.type = "button";
  back.className = "back";
  back.textContent = "‹ pem 키";
  back.addEventListener("click", onBack);

  form.append(heading, body, actions);
  mount.replaceChildren(back, form);
  if (!ready) create.disabled = true;
}
