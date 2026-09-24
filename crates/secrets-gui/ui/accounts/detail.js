// 우측 상세 — 계정 하나가 무엇이고 지금 어떤 상태인가.

import { button, facts, pane, placeholder, span } from "../dom.js";
import { termWrite } from "../terminal.js";
import { expiryText, providerLabelOf, refOf, select } from "./state.js";

const { invoke } = window.__TAURI__.core;

export const mount = document.getElementById("detail");

/* ── 상세 ───────────────────────────────────────────── */

// 이 계정에 할 수 있는 일. 머리말 오른쪽에 모아 둔다.
export function headActions(acc) {
  const box = document.createElement("div");
  box.className = "head-actions";

  box.append(
    button("다시 검증", {
      onClick: () => invoke("verify_account", { provider: acc.provider, slug: acc.slug }),
    }),
  );

  // 기한이 없는 자격도 회전할 수 있어야 하므로 늘 열어 둔다.
  box.append(button("재발급", { onClick: () => openReissue(acc) }));

  if (acc.is_active) {
    box.append(
      button("해제", {
        onClick: () => invoke("deactivate_provider", { provider: acc.provider }),
      }),
    );
  } else if (acc.global_path) {
    box.append(
      button("할당", {
        primary: true,
        onClick: () =>
          invoke("activate_account", { provider: acc.provider, slug: acc.slug }).catch((err) =>
            termWrite("err", String(err)),
          ),
      }),
    );
  }

  box.append(deleteButton(acc));
  return box;
}

// 되돌릴 수 있게 실물은 보관하지만, 사용자가 고르는 건 삭제 여부다.
// 보관은 우리가 늘 하는 일이므로 묻지 않는다.
export function deleteButton(acc) {
  const el = document.createElement("button");
  el.type = "button";
  el.className = "danger";
  el.textContent = "삭제";

  let armed = false;
  const disarm = () => {
    armed = false;
    el.textContent = "삭제";
    el.classList.remove("armed");
  };

  el.addEventListener("click", async () => {
    if (!armed) {
      armed = true;
      el.textContent = acc.is_active ? "할당 해제하고 삭제" : "삭제 확인";
      el.classList.add("armed");
      // 실수로 눌렀다면 그냥 두면 된다.
      setTimeout(disarm, 4000);
      return;
    }

    disarm();
    try {
      await invoke("archive_account", { provider: acc.provider, slug: acc.slug });
      select(null);
    } catch (err) {
      termWrite("err", String(err));
    }
  });
  return el;
}

// AWS 계정 상태. root 관련은 우리가 다루지 않지만 상태는 알려 준다.
export function awsPane(acc) {
  if (acc.root_keys_present === null && acc.root_mfa === null) return null;

  const rows = [];
  if (acc.root_keys_present !== null) {
    rows.push(["root 키", acc.root_keys_present ? "있음" : "없음"]);
  }
  if (acc.root_mfa !== null) {
    rows.push(["root MFA", acc.root_mfa ? "켜짐" : "꺼짐"]);
  }

  const box = pane("계정 상태", facts(rows));

  // root 자격은 이 도구가 보관하지 않는다. 문제가 있을 때만 말한다.
  const problems = [];
  if (acc.root_keys_present) {
    problems.push("root 액세스 키가 있습니다. AWS에서는 삭제를 권장합니다.");
  }
  if (acc.root_mfa === false) {
    problems.push("root MFA가 비활성화되어 있습니다.");
  }
  for (const text of problems) {
    const p = document.createElement("p");
    p.className = "problem";
    p.textContent = text;
    box.append(p);
  }
  return box;
}

// 전역 적용 상태. 버튼은 머리말로 올라갔고 여기엔 사실만 남는다.
export function activePane(acc) {
  const rows = [["설정 홈", acc.cli_home, true]];
  if (acc.global_path) rows.push(["전역 설정", acc.global_path, true]);
  if (acc.git_email) rows.push(["커밋 이메일", acc.git_email, true]);

  const box = pane("격리", facts(rows));

  // 주의가 필요할 때만 말한다. 평소 동작은 설명하지 않는다.
  if (acc.caution) {
    const caution = document.createElement("p");
    caution.className = "problem";
    caution.textContent = acc.caution;
    box.append(caution);
  }
  return box;
}

export function renderAccount(acc) {
  const head = document.createElement("div");
  head.className = "detail-head";

  const titleWrap = document.createElement("div");
  titleWrap.className = "detail-title-wrap";
  titleWrap.append(span("cap", providerLabelOf(acc.provider)));

  const line = document.createElement("div");
  line.className = "detail-title-line";
  const h2 = document.createElement("h2");
  h2.textContent = acc.slug;
  line.append(h2);
  // 지금 이 계정으로 gh 명령이 나가는지. 제목 옆이 제일 먼저 눈에 든다.
  if (acc.is_active) line.append(span("badge-active", "사용 중"));
  titleWrap.append(line);

  if (acc.display) titleWrap.append(span("detail-sub", acc.display));
  head.append(titleWrap);
  head.append(headActions(acc));

  const body = document.createElement("div");
  body.className = "detail-body";

  const verified =
    acc.verified_ok === true
      ? `확인됨 · ${acc.verified_at ?? ""}`
      : acc.verified_ok === false
        ? acc.verified_detail || "확인 실패"
        : "아직 확인하지 않음";

  body.append(
    pane(
      "신원",
      facts(
        [
          ["로그인", acc.identity_name || "미확인", true],
          ["방식", acc.identity_kind || "—"],
          acc.aws_account_id && ["AWS 계정", acc.aws_account_id, true],
          ["검증", verified],
        ].filter(Boolean),
      ),
    ),
  );

  // 만료는 검증보다 위에 둔다. 기한이 지나면 나머지가 다 의미를 잃는다.
  const expiryPane = pane(
    "자격 기한",
    facts([
      ["만료일", acc.expiry === "never" ? "없음" : acc.expires || "확인 안 됨"],
      ["상태", expiryText(acc)],
    ]),
  );
  if (acc.expiry === "never") {
    const warn = document.createElement("p");
    warn.className = "pane-note";
    // 무기한 자격은 유출돼도 스스로 만료되지 않는다. 알림은 안 띄우되 짚어는 둔다.
    warn.textContent =
      "만료 기한이 없는 자격 증명입니다. 유출 시 자동으로 만료되지 않으므로 정기적으로 교체하세요.";
    expiryPane.append(warn);
  }
  if (acc.expiry === "soon" || acc.expiry === "expired") {
    const hint = document.createElement("p");
    hint.className = "pane-note";
    hint.textContent = acc.renewal_hint;
    expiryPane.append(hint);
  }
  body.append(expiryPane);

  const aws = awsPane(acc);
  if (aws) body.append(aws);

  body.append(activePane(acc));
  mount.replaceChildren(head, body);
}

// 자격 교체. 계정은 그대로 두고 값만 갈아 끼운다.
export function openReissue(acc) {
  select({ kind: "reissue", ref: refOf(acc) });
}
