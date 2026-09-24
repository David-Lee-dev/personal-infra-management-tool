// 고르기와 직접 입력을 같이 하는 칸.
//
// 목록은 창으로 띄운다. 칸 아래에 붙이면 스크롤 영역에 잘리고, 이 창이 쓰는
// 엔진은 `<datalist>` 를 그리지도 않는다. 목록에 없는 값을 막지 않는다 —
// IP 를 바로 쳐도 그대로 쓰인다.

import { span } from "./dom.js";
import { modal } from "./modal.js";

/// 입력칸 옆에 목록 여는 버튼을 단다.
export function chooser(input, { title, load }) {
  const open = document.createElement("button");
  open.type = "button";
  open.className = "chooser";
  open.textContent = "목록";
  open.title = title;

  open.addEventListener("click", () => {
    modal(title, (close) => [pickList(load, (value) => {
      input.value = value;
      // 이 값을 보고 다시 그리는 쪽이 있다. 사람이 친 것과 같게 알린다.
      input.dispatchEvent(new Event("input", { bubbles: true }));
      close();
      input.focus();
    })]);
  });
  return open;
}

function pickList(load, pick) {
  const box = document.createElement("div");
  box.className = "picker";

  const search = document.createElement("input");
  search.type = "text";
  search.className = "picker-search";
  search.placeholder = "찾기";
  search.autocomplete = "off";
  search.spellcheck = false;

  const list = document.createElement("div");
  list.className = "picker-list";

  let all = [];
  let shown = [];

  const draw = () => {
    const typed = search.value.trim().toLowerCase();
    shown = typed
      ? all.filter(
          (o) =>
            o.value.toLowerCase().includes(typed) ||
            (o.detail ?? "").toLowerCase().includes(typed),
        )
      : all;

    list.replaceChildren();
    if (!shown.length) {
      list.append(span("picker-none", all.length ? "검색 결과가 없습니다" : "등록된 호스트가 없습니다"));
      return;
    }
    for (const option of shown) {
      const row = document.createElement("button");
      row.type = "button";
      row.className = "picker-row";
      row.append(span("picker-value", option.value));
      if (option.detail) row.append(span("picker-detail", option.detail));
      row.addEventListener("click", () => pick(option.value));
      list.append(row);
    }
  };

  load().then((options) => {
    all = options;
    draw();
  });

  search.addEventListener("input", draw);
  search.addEventListener("keydown", (event) => {
    // 쳐서 좁힌 뒤 Enter 면 맨 위를 고른다. 마우스로 옮겨 갈 이유가 없다.
    if (event.key === "Enter" && shown.length) {
      event.preventDefault();
      pick(shown[0].value);
    }
  });

  draw();
  box.append(search, list);
  return box;
}
