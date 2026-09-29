// 계정으로 접속 — Ghostty 새 창을 열거나, 같은 명령을 클립보드에 넣는다.

import { termWrite } from "../terminal.js";
import { invoke } from "../ipc.js";

/// Ghostty 새 창에서 그 계정으로 들어간다. 실패하면 작업 로그에 이유를 남기고 던진다.
export async function connect(server, login) {
  try {
    const target = await invoke("connect_server_account", { id: server, login });
    termWrite("out", `Ghostty 새 창 — ${target}`);
  } catch (err) {
    termWrite("err", `${login} 접속: ${err}`);
    throw err;
  }
}

/// 그 계정으로 들어가는 ssh 명령을 클립보드에 넣는다. 넣지 못하면 작업 로그에 명령을 적는다.
export async function copyCommand(server, login) {
  const line = await invoke("server_ssh_command", { id: server, login }).catch((err) => {
    termWrite("err", String(err));
    throw err;
  });
  try {
    await navigator.clipboard.writeText(line);
    return true;
  } catch {
    termWrite("out", line);
    return false;
  }
}

/// 계정 칩 — 누르면 접속한다. 관리자는 진하게, 키가 pem · 파일 · 기본 키면 꼬리표를 붙인다.
export function accountChip(server, account) {
  const chip = document.createElement("button");
  chip.type = "button";
  chip.className = "sv-account" + (account.role === "admin" ? " admin" : "") + (account.state === "verified" ? "" : " unverified");
  const tail = { pem: "pem", file: "파일 키", agent: "기본 키" }[account.key_kind];
  const mark = document.createElement("span");
  mark.className = "sv-prompt";
  mark.setAttribute("aria-hidden", "true");
  mark.textContent = "›_";
  const name = document.createElement("span");
  name.className = "mono";
  name.textContent = account.login;
  chip.append(mark, name);
  if (tail) {
    const t = document.createElement("span");
    t.className = "sv-account-tail";
    t.textContent = tail;
    chip.append(t);
  }
  const state = { verified: "", unverified: " · 접속 확인 전", installed: " · 접속 확인 안 됨", local: " · 서버에 없음" }[account.state] ?? "";
  chip.title = `${account.role === "admin" ? "관리자(sudo)" : "사용자"}${state} — 눌러서 Ghostty로 접속`;
  chip.addEventListener("click", async (event) => {
    event.stopPropagation();
    chip.disabled = true;
    try {
      await connect(server, account.login);
    } catch {
      /* 이유는 작업 로그에 남았다 */
    } finally {
      chip.disabled = false;
    }
  });
  return chip;
}
