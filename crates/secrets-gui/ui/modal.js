// 따로 떼어 보여 주는 창.
//
// 한 번 하고 마는 일에 쓴다. 상세 화면에 늘 펼쳐 두면 자리를 차지하고,
// 좁은 칸 안에서는 목록 같은 것이 잘린다.

import { span } from "./dom.js";

/// 창을 띄운다. 닫는 함수를 돌려준다.
export function modal(title, build) {
  const backdrop = document.createElement("div");
  backdrop.className = "backdrop";

  const box = document.createElement("div");
  box.className = "modal";
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-modal", "true");
  box.setAttribute("aria-label", title);

  const head = document.createElement("div");
  head.className = "modal-head";
  head.append(span("cap", title));

  const shut = document.createElement("button");
  shut.type = "button";
  shut.className = "modal-close";
  shut.textContent = "✕";
  shut.setAttribute("aria-label", "닫기");
  head.append(shut);

  const body = document.createElement("div");
  body.className = "modal-body";

  function close() {
    document.removeEventListener("keydown", onKey);
    backdrop.remove();
  }

  function onKey(event) {
    if (event.key === "Escape") close();
  }

  shut.addEventListener("click", close);
  // 바깥을 누르면 닫는다. 안쪽에서 시작한 드래그가 바깥에서 끝나도 닫히지 않게
  // target 을 정확히 본다.
  backdrop.addEventListener("mousedown", (event) => {
    if (event.target === backdrop) close();
  });
  document.addEventListener("keydown", onKey);

  body.append(...build(close));
  box.append(head, body);
  backdrop.append(box);
  document.body.append(backdrop);

  // 첫 입력칸에 손이 가 있게 한다.
  body.querySelector("input")?.focus();
  return close;
}
