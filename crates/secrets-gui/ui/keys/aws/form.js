// 키페어 등록.
//
// pem 키가 어디 있는지는 **사용자가 말한다.** 어느 키페어인지는 AWS 에 그 리전의
// 키페어를 물어 지문으로 가린다 — 받은 pem 의 이름을 바꿔 두는 일이 흔해서, 파일
// 이름은 단서일 뿐이다.
//
// 들이면 원본은 금고로 **옮긴다.** 사본을 남기면 흩어진 상태가 그대로다.

import { button, span } from "../../dom.js";
import { termWrite } from "../../terminal.js";
import { select } from "../state.js";

const { invoke } = window.__TAURI__.core;

export function field(label, id, placeholder, value = "") {
  const wrap = document.createElement("div");
  wrap.className = "field";

  const el = document.createElement("label");
  el.textContent = label;
  el.htmlFor = id;

  const input = document.createElement("input");
  input.id = id;
  input.type = "text";
  input.autocomplete = "off";
  input.spellcheck = false;
  input.placeholder = placeholder;
  input.value = value;

  wrap.append(el, input);
  return { wrap, input };
}

export function choice(label, name, options) {
  const wrap = document.createElement("div");
  wrap.className = "field";
  wrap.append(span("field-label", label));

  const row = document.createElement("div");
  row.className = "choice-row";
  const inputs = [];
  for (const [index, option] of options.entries()) {
    const item = document.createElement("label");
    item.className = "choice";
    const input = document.createElement("input");
    input.type = "radio";
    input.name = name;
    input.value = option.value;
    if (index === 0) input.checked = true;
    item.append(input, span("choice-text", option.label));
    row.append(item);
    inputs.push(input);
  }

  wrap.append(row);
  return { wrap, value: () => inputs.find((i) => i.checked)?.value, inputs };
}

export function renderRegister(mount, account) {
  const machine = choice("종류", "a-machine", [
    { value: "ec2", label: "EC2" },
    { value: "lightsail", label: "Lightsail" },
  ]);
  const region = field("리전", "a-region", "ap-northeast-2", "ap-northeast-2");

  // 경로와 파일 이름을 나눠 받는다. AWS 는 `<키페어>.pem` 으로 내려 주므로 파일
  // 이름을 먼저 키페어 이름으로 짚어 본다.
  const folder = field("pem 키 위치", "a-folder", "~/Downloads");
  const file = field("pem 키 이름", "a-file", "tuk-key");
  const purpose = field("용도", "a-purpose", "비워 둘 수 있습니다");

  const verdict = span("resolved", "");
  const warning = span("problem", "");
  warning.hidden = true;

  // 확인된 것. 누르기 전까지는 비어 있다.
  let checked = null;

  const verify = button("확인", { onClick: () => check() });
  const create = button("들이기", { primary: true, onClick: () => adopt() });
  create.disabled = true;

  /// 친 값에서 실제 파일 자리와 AWS 의 키페어 이름을 읽는다.
  function target() {
    const dir = folder.input.value.trim().replace(/\/+$/, "");
    const named = file.input.value.trim();
    if (!dir || !named || !region.input.value.trim()) return null;

    // `.pem` 은 파일의 확장자이지 AWS 의 이름이 아니다.
    const bare = named.replace(/\.pem$/i, "");
    return { path: `${dir}/${bare}.pem`, name: bare };
  }

  // 확인은 AWS 를 타므로 사용자가 누를 때만 돈다. 칠 때마다 부르지 않는다.
  async function check() {
    const at = target();
    if (!at) {
      verdict.textContent = "종류 · 리전 · pem 키 위치와 이름을 채우세요";
      verdict.className = "resolved bad";
      return;
    }
    verify.disabled = true;
    verify.textContent = "AWS 에 묻는 중…";
    checked = null;
    create.disabled = true;

    try {
      const seen = await invoke("check_private_key", {
        account,
        machine: machine.value(),
        region: region.input.value.trim(),
        name: at.name,
        path: at.path,
      });
      checked = { ...seen, at };
      const renamed = seen.name !== at.name ? ` (파일 이름과 다름)` : "";
      verdict.textContent = seen.verified
        ? `→ ${at.path} · 키페어 ${seen.name}${renamed} 의 것이 맞습니다`
        : `→ ${at.path} · 키페어 ${seen.name} · AWS 가 지문을 주지 않아 맞춰 보지 못했습니다`;
      verdict.className = seen.verified ? "resolved on" : "resolved bad";
      create.disabled = false;

      const hosts = seen.referred_by;
      warning.hidden = !hosts.length;
      warning.textContent = hosts.length
        ? `~/.ssh/config 의 ${hosts.join(" · ")} 가 이 파일을 가리킵니다. 금고로 옮기면 그 접속이 끊깁니다.`
        : "";
    } catch (err) {
      verdict.textContent = String(err);
      verdict.className = "resolved bad";
      warning.hidden = true;
    } finally {
      verify.disabled = false;
      verify.textContent = "확인";
    }
  }

  // 무엇이든 바뀌면 확인은 무효다. 옛 확인으로 들이면 엉뚱한 키가 들어간다.
  const stale = () => {
    checked = null;
    create.disabled = true;
    verdict.textContent = "";
    warning.hidden = true;
  };
  for (const input of [region.input, folder.input, file.input]) {
    input.addEventListener("input", stale);
  }
  for (const input of machine.inputs) input.addEventListener("change", stale);

  async function adopt() {
    create.disabled = true;
    try {
      await invoke("adopt_key_pair", {
        adopt: {
          account: checked.account_id,
          machine: machine.value(),
          region: region.input.value.trim(),
          name: checked.name,
          path: checked.at.path,
          purpose: purpose.input.value.trim(),
          expected: checked.verified ? checked.fingerprint : null,
        },
      });
      select(null);
    } catch (err) {
      termWrite("err", String(err));
      create.disabled = false;
    }
  }

  const form = document.createElement("form");
  form.className = "account-form";
  form.autocomplete = "off";
  form.addEventListener("submit", (event) => event.preventDefault());

  const heading = document.createElement("div");
  heading.className = "detail-head";
  const wrap = document.createElement("div");
  wrap.className = "detail-title-wrap";
  const h2 = document.createElement("h2");
  h2.textContent = "pem 키 등록";
  wrap.append(h2);
  heading.append(wrap);

  // 위치와 이름은 한 줄에. `경로 / 이름.pem` 이 실제 파일 자리 그대로다.
  const where = document.createElement("div");
  where.className = "field";
  where.append(span("field-label", "pem 키"));

  const row = document.createElement("div");
  row.className = "pem-row";
  row.append(folder.input, span("pem-sep", "/"), file.input, span("pem-suffix", ".pem"), verify);
  where.append(row);

  const body = document.createElement("div");
  body.className = "detail-body";
  body.append(machine.wrap, region.wrap, where, verdict, warning, purpose.wrap);

  const actions = document.createElement("div");
  actions.className = "detail-actions";
  actions.append(button("취소", { onClick: () => select(null) }), create);

  const backLink = document.createElement("button");
  backLink.type = "button";
  backLink.className = "back";
  backLink.textContent = "‹ pem 키";
  backLink.addEventListener("click", () => select(null));

  form.append(heading, body, actions);
  mount.replaceChildren(backLink, form);
}
