// 서버 키(pem) 상세 — 쓰는 곳, 그 키로 만든 접속 계정, 파일과 AWS 기록.

import { path, span } from "../../dom.js";
import { back, head } from "../parts.js";
import { block, slots } from "../kit.js";
import { projectChip, usesOf } from "../usage.js";
import { accountsPane } from "./accounts.js";
import { MACHINE_LABEL, pemId } from "./list.js";

function usageBlock(key) {
  const uses = usesOf(pemId(key));
  if (!uses.length) {
    return block("쓰는 곳", {}, span("kd-empty", "이 키페어의 인스턴스에 연결된 프로젝트 환경이 없습니다."));
  }
  const list = span("kd-uses", "");
  for (const u of uses) {
    const row = span("kd-use", "");
    row.append(projectChip(u), span("kd-use-where", `서버 ${u.host ?? ""} · ${u.purpose ?? ""} 계정으로 배포`));
    list.append(row);
  }
  return block("쓰는 곳", { note: "이 키페어의 인스턴스에 연결된 환경" }, list);
}

export function renderKey(mount, key, accounts = [], hooks = {}) {
  const body = document.createElement("div");
  body.className = "kd-body";
  body.append(
    usageBlock(key),
    accountsPane(key, accounts, {
      onCreate: hooks.onCreate ?? (() => {}),
      onOpen: hooks.onOpen ?? (() => {}),
    }),
    block(
      "키",
      {},
      slots([
        ["용도", key.purpose || span("muted", "없음")],
        ["키페어", span("mono", key.name)],
        ["AWS", `${MACHINE_LABEL[key.machine] ?? key.machine} · ${key.region} · 계정 ${key.account}`],
        ["AWS 대조", key.verified ? "키페어 지문과 일치" : span("warn-text", "확인되지 않음")],
        ["지문", span("mono small", key.fingerprint)],
        ["파일", path(`${key.path}/key`)],
        ["가져온 날", key.adopted_at.slice(0, 10)],
      ]),
    ),
  );

  mount.replaceChildren(
    back("서버 키"),
    head(key.name, `${MACHINE_LABEL[key.machine] ?? key.machine} · ${key.region}`, {}),
    body,
  );
}
