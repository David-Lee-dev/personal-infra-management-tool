// 옛 IAM 들이기.
//
// 금고 밖에서 만든 IAM 사용자를 기록으로 들인다. AWS 는 바꾸지 않는다. 시크릿은 AWS 가
// 다시 주지 않으므로 금고에 없다 — 들인 IAM 으로 하는 일은 소비처 기록, 정리 대상 분류,
// 그리고 한 달 넘게 쓰이지 않았을 때의 삭제다.
//
// 키가 하나이고 관리형 정책이 없는 사용자만 들어온다. 그렇지 않으면 뒷단이 까닭을 말한다.

import { button, span } from "../../dom.js";
import { ask, back } from "../parts.js";
import { select } from "../state.js";
import { row, table } from "../table.js";

export function renderIamAdopt(mount, master) {
  if (!master?.account) {
    mount.replaceChildren(
      back("AWS"),
      span("problem", "AWS 마스터 계정이 없습니다. 계정 관리 탭에서 먼저 등록하세요."),
    );
    return;
  }

  const heading = document.createElement("div");
  heading.className = "detail-head";
  const wrap = document.createElement("div");
  wrap.className = "detail-title-wrap";
  const h2 = document.createElement("h2");
  h2.textContent = "옛 IAM 들이기";
  wrap.append(h2);
  heading.append(wrap, span("detail-sub", `${master.account} · ${master.slug}`));

  const body = document.createElement("div");
  body.className = "detail-body";
  body.append(span("pane-note", "AWS 에서 금고에 없는 IAM 사용자를 찾는 중입니다…"));
  mount.replaceChildren(back("AWS"), heading, body);

  ask("adoptable_iam", { master: master.slug, account: master.account })
    .then((names) => {
      if (!names.length) {
        body.replaceChildren(span("pane-note", "들일 IAM 이 없습니다. AWS 의 사용자가 모두 금고에 있습니다."));
        return;
      }
      body.replaceChildren(
        span("pane-note", "AWS 는 바뀌지 않습니다. 시크릿은 들어오지 않습니다."),
        table(
          [
            { label: "IAM 사용자", width: "70%" },
            { label: "", width: "30%" },
          ],
          names.map((name) => {
            const take = button("들이기", {
              onClick: async () => {
                take.disabled = true;
                try {
                  const user = await ask("adopt_iam", {
                    master: master.slug,
                    at: { account: master.account, name },
                  });
                  select({ kind: "iam", ref: user.ref });
                } catch {
                  take.disabled = false;
                }
              },
            });
            return row([name, { node: take }]);
          }),
        ),
      );
    })
    .catch(() => {
      body.replaceChildren(span("problem", "AWS 에 묻지 못했습니다. 터미널 칸을 보세요."));
    });
}
