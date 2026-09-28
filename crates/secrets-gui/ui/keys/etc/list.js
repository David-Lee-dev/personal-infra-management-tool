// 기타 목록 — 다시 받을 수 없는 파일들. 프로젝트로 묶는다.

import { chip, expiryChips, group, line, matches, nothing, toolbar } from "../kit.js";
import { select } from "../state.js";
import { unplaced, useChips, usesOf } from "../usage.js";
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

export function etcId(item) {
  return "etc:" + (item.ref.startsWith("etc/") ? item.ref.slice(4) : item.ref);
}

export function renderList(mount) {
  const items = etcItems();
  const shown = items.filter((i) => matches(i.name, i.project, i.purpose, i.file.name, kindLabel(i.kind)));
  const parts = [toolbar({ placeholder: "항목 · 그룹 · 파일로 찾기" })];
  if (!shown.length) parts.push(nothing(items.length ? "찾는 항목이 없습니다." : "가져온 파일이 없습니다."));
  for (const project of [...new Set(shown.map((item) => item.project))].sort()) {
    parts.push(
      group(
        project,
        shown
          .filter((i) => i.project === project)
          .map((item) =>
            line({
              title: item.name,
              sub: item.purpose || item.file.name,
              chips: [
                chip(kindLabel(item.kind)),
                ...(item.values.length ? [chip(`값 ${item.values.length}개`)] : []),
                ...expiryChips(item.expiry),
              ],
              uses: useChips(usesOf(etcId(item)), { extra: unplaced(item.consumers, usesOf(etcId(item))) }),
              onClick: () => select({ kind: "etc", ref: item.ref }),
            }),
          ),
      ),
    );
  }
  mount.replaceChildren(...parts);
}
