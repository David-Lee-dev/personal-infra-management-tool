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
