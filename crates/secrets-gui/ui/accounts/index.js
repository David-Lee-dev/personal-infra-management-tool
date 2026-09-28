// 계정 화면의 조립 지점 — 무엇을 보여 줄지 고르고 목록을 읽는다.

import { placeholder, span } from "../dom.js";
import { reportUiError, termWrite } from "../terminal.js";
import { accountOf, known, onChange, select, selected, setKnown } from "./state.js";
import { mount as detail, renderAccount } from "./detail.js";
import { renderRail } from "./rail.js";
import { bindForm } from "./form.js";
import { bindReissue } from "./reissue.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const alertBar = document.getElementById("alerts");
const formTemplate = document.getElementById("tpl-form");

// 폼 원본을 쥔 쪽이 폼을 그린다. 상세 칸은 복제본을 받기만 한다.
function fromTemplate() {
  const form = formTemplate.content.cloneNode(true).querySelector("form");
  detail.replaceChildren(form);
  return form;
}

function renderForm(providerId) {
  bindForm(fromTemplate(), providerId);
}

function renderReissue(acc) {
  bindReissue(fromTemplate(), acc);
}

function renderEmpty() {
  detail.replaceChildren(
    known().length
      ? placeholder("계정을 선택하세요", "왼쪽에서 계정을 선택하면 신원과 격리 상태를 확인할 수 있습니다.")
      : placeholder(
          "등록된 계정이 없습니다",
          "왼쪽 서비스 옆의 ＋ 버튼으로 계정을 연결하세요.",
        ),
  );
}

function renderDetail() {
  const selection = selected();
  if (selection?.kind === "new") return renderForm(selection.provider);

  if (selection?.kind === "reissue") {
    const acc = accountOf(selection.ref);
    if (acc) return renderReissue(acc);
  }

  if (selection?.kind === "account") {
    const acc = accountOf(selection.ref);
    if (acc) return renderAccount(acc);
    // 방금 만든 계정은 목록에 아직 없을 수 있다. 선택을 지우지 않는다 —
    // 지우면 목록이 도착해도 상세가 열리지 않는다. 정리는 loadAccounts 가 한다.
  }
  renderEmpty();
}

function renderAlerts(messages) {
  alertBar.replaceChildren();
  alertBar.hidden = !messages.length;
  for (const message of messages) {
    alertBar.append(span("alert", message));
  }
}

export async function loadAccounts() {
  try {
    const result = await invoke("list_accounts");
    setKnown(result.accounts);
    renderAlerts(result.alerts);

    // 읽지 못한 항목을 조용히 숨기면 계정이 사라진 것처럼 보인다.
    for (const message of result.errors) termWrite("err", message);
  } catch (err) {
    setKnown([]);
    termWrite("err", `계정 목록을 읽지 못했습니다: ${err}`);
  }

  // 선택한 계정이 사라졌으면 선택을 비운다. 폼은 열어 둔 채로 둔다.
  const selection = selected();
  if (selection?.kind === "account" && !accountOf(selection.ref)) {
    select(null);
    return;
  }
  renderRail();
  renderDetail();
}

listen("accounts:updated", loadAccounts);

onChange(() => {
  renderRail();
  renderDetail();
});
