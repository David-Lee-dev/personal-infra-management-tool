// 자격 재발급 폼 — 같은 계정의 새 자격으로 갈아 끼운다.

import { span } from "../dom.js";
import { bindChallenge } from "./challenge.js";
import { providerLabelOf, refOf, select, whenLeaving } from "./state.js";

const { invoke } = window.__TAURI__.core;

/* ── 자격 재발급 ────────────────────────────────────── */

export function bindReissue(form, acc) {
  const fTitle = form.querySelector("#f-title");
  const fGuidance = form.querySelector("#f-guidance");
  const fFields = form.querySelector("#f-fields");
  const fBrowser = form.querySelector("#f-browser");
  const fProbe = form.querySelector("#f-probe");
  const fIdentity = form.querySelector("#f-identity");
  const fDisplay = form.querySelector("#f-display");
  const fSubmit = form.querySelector("#f-submit");
  const fCancel = form.querySelector("#f-cancel");
  const fError = form.querySelector("#f-error");

  form.querySelector(".cap").textContent = "자격 교체";
  fTitle.textContent = `${acc.slug} 재발급`;

  // 설명은 계정에 딸린 것이지 자격에 딸린 것이 아니다. 바꿀 일이 없다.
  fDisplay.closest(".field").hidden = true;

  let spec = null;
  let probed = null;

  function showError(message) {
    fError.textContent = message;
    fError.hidden = !message;
  }

  function collectValues() {
    const values = {};
    for (const input of fFields.querySelectorAll("input")) {
      values[input.dataset.key] = input.value;
    }
    return values;
  }

  // 확인만 하고 쓰지 않기로 한 자격은 버린다. 준비 홈에 로그인이 남아 있다.
  function invalidate() {
    if (probed?.preparation) {
      invoke("discard_preparation", { preparation: probed.preparation }).catch(() => {});
    }
    probed = null;
    fIdentity.hidden = true;
    fSubmit.disabled = true;
  }

  // 새 자격이 같은 계정의 것인지 본다. 아니면 붙이지 않는다.
  function accept(result) {
    if (result.name !== acc.identity_name) {
      invalidate();
      showError(
        `다른 계정의 자격입니다. 이 계정은 ${acc.identity_name} 인데 넣은 자격은 ${result.name} 입니다.`,
      );
      return;
    }
    probed = result;
    showIdentity(result);
    fSubmit.disabled = false;
  }

  whenLeaving(invalidate);

  const challenge = bindChallenge(form, acc.provider, {
    onError: showError,
    onDone: accept,
  });

  async function load() {
    showError("");
    fFields.replaceChildren();
    invalidate();

    try {
      spec = await invoke("provider_form", { provider: acc.provider });
    } catch (err) {
      showError(String(err));
      return;
    }

    // 기한을 늘리는 방법이 없다는 걸 여기서 한 번 더 말한다.
    fGuidance.textContent = `${acc.renewal_hint} 같은 계정(${acc.identity_name})의 자격이어야 합니다.`;

    for (const field of spec.fields) {
      const wrap = document.createElement("div");
      wrap.className = "field";

      const label = document.createElement("label");
      label.htmlFor = `v-${field.key}`;
      label.textContent = field.label + (field.required ? "" : " (선택)");
      wrap.append(label);

      const input = document.createElement("input");
      input.id = `v-${field.key}`;
      input.type = field.secret ? "password" : "text";
      input.dataset.key = field.key;
      input.autocomplete = "off";
      input.spellcheck = false;
      input.addEventListener("input", invalidate);
      wrap.append(input);

      if (field.help) wrap.append(span("field-help", field.help));
      fFields.append(wrap);
    }

    fBrowser.hidden = !spec.browser_url;
    if (spec.browser_url) fBrowser.textContent = spec.browser_label;
    fProbe.textContent = spec.flow !== "credential" ? "브라우저로 다시 로그인" : "자격 확인";
    fProbe.disabled = spec.flow === "credential" && spec.fields.length === 0;
    challenge.reset();
    fFields.querySelector("input")?.focus();
  }

  function showIdentity(result) {
    fIdentity.replaceChildren();
    fIdentity.hidden = false;
    fIdentity.append(span("identity-name", result.name));

    const row = (label, value, cls = "") => {
      const el = document.createElement("div");
      el.className = "identity-row";
      el.append(span("identity-label", label));
      el.append(span(`identity-value ${cls}`.trim(), value));
      return el;
    };

    fIdentity.append(
      row(
        "새 만료",
        result.expires === "never" ? "기한 없음" : (result.expires ?? "확인 못 함"),
      ),
    );
    if (result.scopes.length) {
      fIdentity.append(row("scope", result.scopes.join(", "), "mono wrap"));
    }
  }

  async function probe() {
    // 앞서 확인해 둔 자격이 있으면 먼저 버린다. 새로 확인하면 그것은 쓰이지 않는다.
    invalidate();
    showError("");
    fProbe.disabled = true;
    fProbe.textContent = spec.flow !== "credential" ? "브라우저에서 진행하세요…" : "확인 중…";

    try {
      // 코드를 받아 와야 끝나는 경우는 여기서 멈추고 입력을 기다린다.
      if (spec.flow === "browser-code") {
        await challenge.begin();
        return;
      }

      accept(
        spec.flow !== "credential"
          ? await invoke("probe_browser", { provider: acc.provider })
          : await invoke("probe_credentials", {
              provider: acc.provider,
              values: collectValues(),
            }),
      );
    } catch (err) {
      invalidate();
      showError(String(err));
    } finally {
      fProbe.disabled = false;
      fProbe.textContent = spec.flow !== "credential" ? "브라우저로 다시 로그인" : "자격 확인";
    }
  }

  fProbe.addEventListener("click", probe);
  fCancel.addEventListener("click", () => select({ kind: "account", ref: refOf(acc) }));

  fBrowser.addEventListener("click", () => {
    if (spec?.browser_url) {
      invoke("open_url", { url: spec.browser_url }).catch((err) => showError(String(err)));
    }
  });

  fSubmit.textContent = "교체";
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!probed) return;
    showError("");
    fSubmit.disabled = true;

    try {
      // 확인 단계가 붙여 둔 자격을 그대로 갈아 끼운다. 비밀값을 다시 보내지 않는다.
      await invoke("replace_credential", {
        provider: acc.provider,
        slug: acc.slug,
        preparation: probed.preparation,
      });
      select({ kind: "account", ref: refOf(acc) });
    } catch (err) {
      showError(String(err));
      fSubmit.disabled = false;
    }
  });

  load();
}
