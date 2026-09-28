// 서버 키 목록 — 이 금고가 들인 pem 키. 인스턴스에 처음 들어가는 문이다.
//
// 한 줄에 키페어 · 종류 · 리전, 그 키로 들어가는 서버, 그 서버에 연결된 프로젝트 환경.

import { span } from "../../dom.js";
import { chip, expiryChips, group, line, matches, nothing, toolbar } from "../kit.js";
import { select } from "../state.js";
import { useChips, usesOf } from "../usage.js";
import { known, serversFor } from "./state.js";

export const MACHINE_LABEL = { ec2: "EC2", lightsail: "Lightsail" };

export function pemId(key) {
  return `pem:${key.region}/${key.name}`;
}

export function renderList(mount) {
  const keys = known();
  const shown = keys.filter((key) => matches(key.name, key.purpose, key.region));
  const parts = [toolbar({ placeholder: "키페어 · 용도로 찾기" })];
  if (!shown.length) {
    parts.push(nothing(keys.length ? "찾는 키가 없습니다." : "＋ pem 키 등록으로 로컬에 있는 pem 키를 금고로 옮기세요."));
  } else {
    parts.push(
      group(
        "pem 키",
        shown.map((key) => {
          const servers = serversFor(key);
          const chips = [chip(MACHINE_LABEL[key.machine] ?? key.machine), chip(key.region)];
          if (servers.length) chips.push(chip(`서버 ${servers.length}대`));
          if (!key.verified) chips.push(chip("AWS 대조 안 됨", "warn"));
          chips.push(...expiryChips(key.expiry));
          return line({
            title: key.name,
            sub: key.purpose || "용도 없음",
            chips,
            uses: useChips(usesOf(pemId(key))),
            tone: key.verified ? "" : "warn",
            onClick: () => select({ kind: "aws-key", ref: key.ref }),
          });
        }),
      ),
    );
  }
  mount.replaceChildren(...parts, span("kl-foot", "서버에 접속하는 계정은 서버 메뉴에서 만들고 관리합니다."));
}
