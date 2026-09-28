// 서버 키(pem) 상세 — 쓰는 곳, 이 키로 들어가는 서버, 파일과 AWS 기록.

import { path, span } from "../../dom.js";
import { ask, back, expiryField, head } from "../parts.js";
import { block, slots } from "../kit.js";
import { projectChip, usesOf } from "../usage.js";
import { openServer } from "../../servers/index.js";
import { MACHINE_LABEL, pemId } from "./list.js";
import { serversFor } from "./state.js";

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

/// 이 pem 으로 들어가는 계정이 있는 서버. 계정 만들기 · 관리는 서버 화면에서 한다.
function serversBlock(key) {
  const found = serversFor(key);
  const note = "서버 계정 만들기 · 관리는 서버 메뉴의 서버 상세에서 합니다.";
  if (!found.length) {
    return block("이 키로 들어가는 서버", { note }, span("kd-empty", "이 pem 으로 들어가는 계정이 있는 서버가 없습니다."));
  }
  const list = span("kd-uses", "");
  for (const { server, logins } of found) {
    const row = span("kd-use", "");
    const link = document.createElement("button");
    link.type = "button";
    link.className = "link-button strong mono";
    link.textContent = server.name;
    link.addEventListener("click", () => openServer(server.id));
    row.append(link, span("kd-use-where mono", `${server.address} · ${logins.join(", ")}`));
    list.append(row);
  }
  return block("이 키로 들어가는 서버", { note }, list);
}

export function renderKey(mount, key) {
  const body = document.createElement("div");
  body.className = "kd-body";
  body.append(
    usageBlock(key),
    serversBlock(key),
    block(
      "키",
      {},
      slots([
        ["용도", key.purpose || span("muted", "없음")],
        [
          "만료",
          expiryField(key.expiry, (to) =>
            ask("set_pem_expires", {
              at: { account: key.account, machine: key.machine, region: key.region, name: key.name },
              to,
            }),
          ),
        ],
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
