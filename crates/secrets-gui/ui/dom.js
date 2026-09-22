// 화면을 만드는 데 되풀이되는 조각.

export function span(className, text) {
  const el = document.createElement("span");
  el.className = className;
  el.textContent = text;
  return el;
}

export function cell(node) {
  const td = document.createElement("td");
  td.append(node);
  return td;
}
