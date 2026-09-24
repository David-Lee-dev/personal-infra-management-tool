// AWS 목록 — 이 금고가 들인 pem 키와 IAM 사용자.
//
// pem 이 위다. 서버에 들어가는 문이라 잃으면 복구가 가장 어렵다.

import { span } from "../../dom.js";
import { select } from "../state.js";
import { iamSection } from "./iam.js";
import { known } from "./state.js";
import { row, section, table } from "../table.js";

const MACHINE_LABEL = { ec2: "EC2", lightsail: "Lightsail" };

function pemSection() {
  const keys = known();
  if (!keys.length) {
    return [
      section("pem 키", 0),
      span("list-none", "＋ pem 키 등록 을 눌러 이 맥에 흩어진 pem 키를 금고로 옮기세요."),
    ];
  }
  return [
    section("pem 키", keys.length),
    table(
      [
        { label: "키페어", width: "34%" },
        { label: "용도", width: "30%" },
        { label: "종류", width: "14%" },
        { label: "리전", width: "22%" },
      ],
      keys.map((key) =>
        row(
          [
            key.name,
            key.purpose,
            { node: span("chip", MACHINE_LABEL[key.machine] ?? key.machine) },
            key.region,
          ],
          () => select({ kind: "aws-key", ref: key.ref }),
        ),
      ),
    ),
  ];
}

export function renderList(mount) {
  mount.replaceChildren(...pemSection(), ...iamSection());
}
