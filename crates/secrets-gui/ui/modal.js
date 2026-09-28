// 따로 떼어 보여 주는 창.
//
// 한 번 하고 마는 일에 쓴다. 상세 화면에 늘 펼쳐 두면 자리를 차지하고,
// 좁은 칸 안에서는 목록 같은 것이 잘린다.
//
// 창은 내용에 따라 출렁이지 않는다.
// - 폭은 내용의 종류로 고른다(size: sm · md · lg · xl). 화면이 좁으면 화면에 맞춘다.
// - 버튼 줄(.modal-actions)은 본문 밖 아래에 고정한다. 내용이 바뀌어도 버튼 자리가 그대로다.
// - 보통 창은 한 번 커진 높이 아래로 줄지 않는다. 늘어나다 화면 끝에 닿으면 본문만 스크롤된다.
// - fill 창은 처음부터 화면 높이를 쓴다. 그 안에서 한 영역만 스크롤되게 짠다(편집기 · 로그).

import { span } from "./dom.js";

/// 창을 띄운다. 닫는 함수를 돌려준다.
export function modal(title, build, { size = "md", fill = false } = {}) {
  const opener = document.activeElement;
  const background = [...document.querySelectorAll(".shell, .backdrop")].map((node) => [node, node.inert]);
  const backdrop = document.createElement("div");
  backdrop.className = "backdrop";

  const box = document.createElement("div");
  box.className = `modal size-${size}` + (fill ? " fill" : "");
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

  let closed = false;
  function close() {
    if (closed) return;
    closed = true;
    watcher.disconnect();
    keeper.disconnect();
    document.removeEventListener("keydown", onKey);
    backdrop.remove();
    for (const [node, inert] of background) node.inert = inert;
    if (opener?.isConnected) opener.focus();
  }

  function onKey(event) {
    if (backdrop.inert) return;
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
    if (event.key !== "Tab") return;
    const focusable = [...box.querySelectorAll('button, input, select, textarea, a[href], summary, [tabindex]')]
      .filter((node) => !node.disabled && node.tabIndex >= 0 && node.getClientRects().length);
    const first = focusable[0] ?? shut;
    const last = focusable[focusable.length - 1] ?? shut;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  shut.addEventListener("click", close);
  // 바깥을 누르면 닫는다. 안쪽에서 시작한 드래그가 바깥에서 끝나도 닫히지 않게
  // target 을 정확히 본다.
  backdrop.addEventListener("mousedown", (event) => {
    if (event.target === backdrop) close();
  });
  document.addEventListener("keydown", onKey);

  const foot = document.createElement("div");
  foot.className = "modal-foot";
  foot.hidden = true;

  // 버튼 줄은 본문에서 꺼내 아래에 둔다. 내용을 나중에 그리는 창도 있어 생길 때마다 옮긴다.
  function liftActions() {
    const actions = body.querySelector(".modal-actions");
    if (!actions) return;
    foot.replaceChildren(actions);
    foot.hidden = false;
  }
  const watcher = new MutationObserver(liftActions);
  watcher.observe(body, { childList: true, subtree: true });

  // 한 번 커진 높이 아래로 줄지 않게 한다. 화면보다 크게 잡지는 않는다.
  let tallest = 0;
  const keeper = new ResizeObserver(() => {
    if (fill) return;
    const height = box.getBoundingClientRect().height;
    const room = backdrop.clientHeight - 64;
    if (height > tallest) {
      tallest = Math.min(height, room);
      box.style.minHeight = `${tallest}px`;
    }
  });

  body.append(...build(close));
  box.append(head, body, foot);
  backdrop.append(box);
  for (const [node] of background) node.inert = true;
  document.body.append(backdrop);
  liftActions();
  keeper.observe(box);

  // 첫 입력칸에 손이 가 있게 한다.
  (body.querySelector('input:not([disabled]):not([type="hidden"])') ?? shut).focus();
  return close;
}
