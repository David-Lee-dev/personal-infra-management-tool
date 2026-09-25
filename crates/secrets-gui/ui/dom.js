// 화면을 만드는 데 되풀이되는 조각.

export function span(className, text) {
  const el = document.createElement("span");
  el.className = className;
  el.textContent = text;
  return el;
}

// 경로는 `/` 에서만 접힌다. 그냥 두면 단어 한가운데가 잘려 읽을 수 없다.
export function path(text) {
  const el = document.createElement("span");
  el.className = "path";
  const parts = text.split("/");
  for (const [index, part] of parts.entries()) {
    el.append(index === parts.length - 1 ? part : `${part}/`);
    if (index < parts.length - 1) el.append(document.createElement("wbr"));
  }
  return el;
}

export function cell(node) {
  const td = document.createElement("td");
  td.append(node);
  return td;
}

/* ── 상세 칸의 조각 ──────────────────────────────────── */

export function pane(title, ...children) {
  const box = document.createElement("section");
  box.className = "pane";

  const head = document.createElement("div");
  head.className = "pane-head";
  head.append(span("cap", title));
  box.append(head);

  box.append(...children);
  return box;
}

export function facts(pairs) {
  const list = document.createElement("dl");
  list.className = "facts";
  for (const [label, value, mono] of pairs) {
    const dt = document.createElement("dt");
    dt.textContent = label;
    const dd = document.createElement("dd");
    if (mono) dd.className = "mono";
    if (value instanceof Node) dd.append(value);
    else dd.textContent = value;
    list.append(dt, dd);
  }
  return list;
}

export function button(label, { primary = false, onClick } = {}) {
  const el = document.createElement("button");
  el.type = "button";
  el.textContent = label;
  if (primary) el.className = "primary";
  if (onClick) el.addEventListener("click", onClick);
  return el;
}

export function placeholder(title, body) {
  const box = document.createElement("div");
  box.className = "placeholder";
  const strong = document.createElement("strong");
  strong.textContent = title;
  const p = document.createElement("p");
  p.textContent = body;
  box.append(strong, p);
  return box;
}

/* ── 고르거나 직접 입력하는 칸 ─────────────────────────── */

/// 고를 값이 있으면 드롭다운으로 보이고, 맨 끝의 "직접 입력"을 고르면 입력칸이 열린다.
/// 고를 값이 없으면 입력칸만 보인다. 목록이 있다는 것을 모양으로 알 수 있게 datalist 대신 쓴다.
///
/// 돌려주는 것: `node` (화면에 붙일 것), `value()` (지금 값), `set(v)` (값 바꾸기 — 목록에 없으면 입력칸으로).
export function pickOrType(options, { newLabel = "＋ 직접 입력", placeholder = "", selected = "", onChange } = {}) {
  const NEW = "\u0000new";
  const node = document.createElement("div");
  node.className = "pick-or-type";

  const input = document.createElement("input");
  input.type = "text";
  input.placeholder = placeholder;
  input.spellcheck = false;
  input.autocomplete = "off";
  input.addEventListener("input", () => onChange?.(input.value));

  const select = document.createElement("select");
  for (const value of options) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = value;
    select.append(option);
  }
  const typed = document.createElement("option");
  typed.value = NEW;
  typed.textContent = newLabel;
  select.append(typed);

  function typing(on) {
    input.hidden = !on;
    if (on) input.focus();
  }
  select.addEventListener("change", () => {
    typing(select.value === NEW);
    onChange?.(select.value === NEW ? input.value : select.value);
  });

  function set(value) {
    if (options.includes(value)) {
      select.value = value;
      typing(false);
    } else {
      select.value = NEW;
      input.value = value;
      input.hidden = false;
    }
  }

  if (options.length) {
    node.append(select, input);
    set(selected && options.includes(selected) ? selected : options[0]);
  } else {
    node.append(input);
    input.value = selected;
  }

  return {
    node,
    value: () => (options.length && select.value !== NEW ? select.value : input.value),
    set,
  };
}
