// 기타 목록 — 다시 받을 수 없는 파일들. 프로젝트로 묶는다.

import { span } from "../../dom.js";
import { select } from "../state.js";
import { groupRow, row, section, table } from "../table.js";
import { kindLabel } from "./kinds.js";
// 금고에서 읽은 항목. 목록을 불러올 때마다 통째로 바뀐다.
let items = [];

export function setEtcItems(next) {
  items = next;
}

export function etcItems() {
  return items;
}

export function etcOf(ref) {
  return items.find((item) => item.ref === ref);
}

const COLUMNS = [
  { label: "항목", width: "26%" },
  { label: "종류", width: "20%" },
  { label: "파일", width: "40%" },
  { label: "소비처", width: "14%" },
];

function fileCell(item) {
  const box = document.createElement("div");
  box.className = "cell-actions";
  const name = span("mono", item.file.name);
  name.title = item.file.name;
  box.append(name);
  if (item.values.length) box.append(span("chip", `비밀번호 ${item.values.length}`));
  return box;
}

export function renderList(mount) {
  const items = etcItems();
  if (!items.length) {
    mount.replaceChildren(
      section("기타", 0),
      span("list-none", "금고에 들인 파일이 없습니다."),
    );
    return;
  }

  const projects = [...new Set(items.map((item) => item.project))].sort();
  const rows = [];
  for (const project of projects) {
    rows.push(groupRow(project, COLUMNS.length));
    for (const item of items.filter((i) => i.project === project)) {
      rows.push(
        row(
          [item.name, kindLabel(item.kind), { node: fileCell(item) }, `${item.consumers.length}곳`],
          () => select({ kind: "etc", ref: item.ref }),
        ),
      );
    }
  }
  mount.replaceChildren(section("기타", items.length), table(COLUMNS, rows));
}
