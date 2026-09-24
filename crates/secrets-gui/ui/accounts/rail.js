// 좌측 계정 목록.

import { span } from "../dom.js";
import { PROVIDERS, expiryText, known, providerLabelOf, refOf, select, selected } from "./state.js";

const mount = document.getElementById("rail");

/* ── 레일 ───────────────────────────────────────────── */

function railItem(acc) {
  const here = selected();
  const button = document.createElement("button");
  button.type = "button";
  button.className = "rail-item";
  button.setAttribute(
    "aria-current",
    String(
      (here?.kind === "account" || here?.kind === "reissue") && here.ref === refOf(acc),
    ),
  );

  // 문제를 레일에서 바로 본다. 만료가 검증 실패보다 급하다.
  const expiring = acc.expiry === "soon" || acc.expiry === "expired";
  const state = expiring
    ? " warn"
    : acc.verified_ok === true
      ? ""
      : acc.verified_ok === false
        ? " warn"
        : " unknown";
  button.append(span(`dot${state}`, ""));
  button.append(span("slug", acc.slug));
  // 지금 전역으로 쓰이는 계정. 터미널에서 치는 명령이 이 계정으로 나간다.
  if (acc.is_active) button.append(span("rail-active", "사용 중"));
  button.title = acc.display || acc.slug;

  button.addEventListener("click", () => select({ kind: "account", ref: refOf(acc) }));
  return button;
}

export function renderRail() {
  mount.replaceChildren();

  for (const provider of PROVIDERS) {
    const group = document.createElement("div");
    group.className = "rail-group";
    group.append(span("cap", provider.label));

    const add = document.createElement("button");
    add.type = "button";
    add.className = "rail-add";
    add.textContent = "＋";
    add.title = `${provider.label} 계정 추가`;
    add.addEventListener("click", () => select({ kind: "new", provider: provider.id }));
    group.append(add);
    mount.append(group);

    const mine = known().filter((a) => a.provider === provider.id);
    if (!mine.length) {
      mount.append(span("rail-none", "없음"));
      continue;
    }
    for (const acc of mine) mount.append(railItem(acc));
  }

  // 항목이 적어도 레일이 위로 뭉치지 않게 남는 공간을 채운다.
  const filler = document.createElement("div");
  filler.className = "rail-filler";
  mount.append(filler);
}
