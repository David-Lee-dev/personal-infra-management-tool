// 들인 pem 키의 상세.

import { button, facts, pane, path, span } from "../../dom.js";
import { termWrite } from "../../terminal.js";
import { back, head, purposeField, side } from "../parts.js";
import { accountsPane } from "./accounts.js";

const MACHINE_LABEL = { ec2: "EC2", lightsail: "Lightsail" };

// 뒷단이 아직 없다. 무엇을 하려 했는지 터미널에 남겨 화면만 먼저 본다.
function pending(what) {
  termWrite("out", `[미구현] ${what}`);
}

export function renderKey(mount, key, accounts = [], hooks = {}) {
  const body = document.createElement("div");
  body.className = "detail-body";
  body.append(
    side(
      pane(
        "파일",
        facts([
          ["pem 키", path(`${key.path}/key`), true],
          ["지문", key.fingerprint, true],
          ["AWS 키페어 대조", key.verified ? "일치함" : "확인되지 않음"],
          ["가져온 날", key.adopted_at.slice(0, 10)],
        ]),
      ),
      pane(
        "AWS",
        facts([
          [
            "용도",
            purposeField(key.purpose, (to) => pending(`용도 → ${to}`)),
          ],
          ["키페어", key.name, true],
          ["종류", MACHINE_LABEL[key.machine] ?? key.machine],
          ["리전", key.region],
          ["계정", key.account, true],
        ]),
      ),
    ),
    accountsPane(key, accounts, {
      onCreate: hooks.onCreate ?? (() => {}),
      onOpen: hooks.onOpen ?? (() => {}),
    }),
  );

  mount.replaceChildren(
    back("pem 키"),
    head(key.name, `${MACHINE_LABEL[key.machine] ?? key.machine} · ${key.region}`, {
      buttons: [
        button("pem 키 내용 복사", { onClick: () => pending(`${key.name} pem 키 복사`) }),
        button("삭제", { onClick: () => pending(`${key.name} 삭제`) }),
      ],
    }),
    body,
  );
}
