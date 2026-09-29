// 등록하지 않은 서버 — 서버 계정 기록과 ~/.ssh/config 에서 찾은 것. 고른 것만 등록한다.

import { button, span } from "../dom.js";
import { modal } from "../modal.js";
import { KIND_LABEL, textInput } from "./form.js";
import { invoke } from "../ipc.js";
import { termWrite } from "../terminal.js";

function row(found) {
  const line = document.createElement("div");
  line.className = "sv-suggest";

  const pick = document.createElement("input");
  pick.type = "checkbox";
  pick.checked = true;
  pick.setAttribute("aria-label", `${found.name} 등록`);

  const name = textInput(found.name);
  name.setAttribute("aria-label", "서버 이름");
  const who = span("sv-suggest-name", "");
  who.append(name, span("mono small muted", found.port === 22 ? found.address : `${found.address}:${found.port}`));

  const accounts = span("sv-accounts", "");
  for (const a of found.accounts) {
    const chip = span(`sv-account static${a.role === "admin" ? " admin" : ""}`, "");
    chip.append(span("mono", a.login), span("sv-account-tail", a.key_label));
    accounts.append(chip);
  }

  const facts = span("sv-suggest-facts", "");
  facts.append(span("chip", KIND_LABEL[found.kind] ?? found.kind));
  facts.append(span("small", `관리 접속: ${found.admin ?? "지정 안 함"}`));
  facts.append(span("small muted", found.sources.join(" · ")));
  if (found.links.length) facts.append(span("small", `이을 환경: ${found.links.join(", ")}`));
  for (const skipped of found.skipped) facts.append(span("small warn-text", `옮기지 않는 설정 — ${skipped}`));

  line.append(pick, who, accounts, facts);
  return { line, value: () => (pick.checked ? { key: found.key, name: name.value.trim() } : null) };
}

export function openSuggestions() {
  modal("등록하지 않은 서버", (close) => {
    const holder = document.createElement("div");
    holder.className = "git-form";
    holder.append(span("muted", "서버 계정 기록과 ~/.ssh/config를 읽는 중…"));
    invoke("server_suggestions").then((found) => {
      if (!found.length) {
        holder.replaceChildren(span("kd-empty", "등록하지 않은 서버가 없습니다."));
        return;
      }
      const rows = found.map(row);
      const result = span("pane-note", "");
      const submit = button(`고른 서버 등록`, { primary: true });
      submit.addEventListener("click", async () => {
        const picks = rows.map((r) => r.value()).filter(Boolean);
        if (!picks.length) return;
        submit.disabled = true;
        const done = await invoke("adopt_servers", { picks });
        for (const message of done.errors) termWrite("err", `✗ 서버 등록 실패 — ${message}`);
        const lines = [`등록: ${done.registered.join(", ") || "없음"}`];
        if (done.linked.length) lines.push(`서버로 이은 환경: ${done.linked.join(", ")}`);
        lines.push(...done.errors);
        result.textContent = lines.join("\n");
        result.classList.toggle("problem", done.errors.length > 0);
        if (!done.errors.length) close();
        else submit.disabled = false;
      });
      const actions = document.createElement("div");
      actions.className = "modal-actions";
      actions.append(button("취소", { onClick: close }), submit);
      holder.replaceChildren(
        span("pane-note", "주소가 같은 기록 · 별칭은 서버 하나로 묶었습니다. 이름은 여기서 고칠 수 있습니다. 키 파일은 옮기지 않고, ~/.ssh/config는 바꾸지 않습니다. 별칭에서 온 계정은 sudo 여부를 알 수 없어 사용자로 적습니다 — 등록한 뒤 역할을 고치세요."),
        ...rows.map((r) => r.line),
        result,
        actions,
      );
    });
    return [holder];
  }, { size: "xl" });
}
