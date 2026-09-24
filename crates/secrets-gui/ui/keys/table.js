// 키 화면의 표 조각. 도메인이 달라도 목록 모양은 같다.

import { span } from "../dom.js";

export function section(title, count) {
  const head = document.createElement("div");
  head.className = "list-head";
  head.append(span("cap", title), span("list-count", String(count)));
  return head;
}

export function table(columns, rows) {
  const el = document.createElement("table");
  el.className = "list";

  const group = document.createElement("colgroup");
  for (const column of columns) {
    const col = document.createElement("col");
    col.style.width = column.width;
    group.append(col);
  }
  const tail = document.createElement("col");
  tail.style.width = "28px";
  group.append(tail);
  el.append(group);

  const headRow = document.createElement("tr");
  for (const column of columns) {
    const th = document.createElement("th");
    th.textContent = column.label;
    headRow.append(th);
  }
  headRow.append(document.createElement("th"));

  const thead = document.createElement("thead");
  thead.append(headRow);
  const body = document.createElement("tbody");
  body.append(...rows);
  el.append(thead, body);
  return el;
}

// 누를 곳이 없는 줄은 `›` 도 없다. 눌리는 것처럼 보이면 안 된다.
export function row(cells, onClick = null) {
  const tr = document.createElement("tr");
  tr.className = onClick ? "list-row" : "list-row flat";

  for (const [index, cell] of cells.entries()) {
    const td = document.createElement("td");
    if (index === 0 && onClick) {
      td.className = "list-name strong";
      const line = document.createElement("div");
      line.className = "name-line";
      line.append(span("dot", ""), span("", cell));
      td.append(line);
    } else if (cell?.node) {
      td.append(cell.node);
    } else {
      td.textContent = cell ?? "";
    }
    tr.append(td);
  }

  const tail = document.createElement("td");
  tail.className = "list-go";
  if (onClick) tail.textContent = "›";
  tr.append(tail);

  if (onClick) {
    tr.tabIndex = 0;
    tr.addEventListener("click", onClick);
    tr.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        onClick();
      }
    });
  }
  return tr;
}

// 묶음의 머리줄. 칸을 가로질러 이름 하나만 둔다.
export function groupRow(label, columns) {
  const tr = document.createElement("tr");
  tr.className = "list-group";
  const td = document.createElement("td");
  td.colSpan = columns + 1;
  td.textContent = label;
  tr.append(td);
  return tr;
}
