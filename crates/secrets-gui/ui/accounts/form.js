// 계정 추가 폼 — 자격을 확인하고 그 자격으로 계정을 만든다.

import { span } from "../dom.js";
import { bindChallenge } from "./challenge.js";
import { providerLabelOf, select, whenLeaving } from "./state.js";
import { invoke } from "../ipc.js";

/* ── 계정 추가 폼 ───────────────────────────────────── */

export function bindForm(form, providerId) {
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

  const providerLabel = providerLabelOf(providerId);
  fTitle.textContent = `${providerLabel} 계정 연결`;

  let spec = null;
  // 확인으로 알아낸 사실. 이름과 만료일은 여기서만 온다.
  let probed = null;

  const challenge = bindChallenge(form, providerId, {
    onError: showError,
    onDone: (result) => {
      probed = result;
      showIdentity(result);
      if (!fDisplay.value.trim()) fDisplay.value = result.display;
      fSubmit.disabled = false;
      fDisplay.focus();
    },
  });

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

  // 자격을 고치면 앞서 확인한 사실은 더 이상 유효하지 않다.
  // 확인만 하고 쓰지 않기로 한 자격은 버린다. 준비 홈에 로그인이 남아 있다.
  function invalidate() {
    if (probed?.preparation) {
      invoke("discard_preparation", { preparation: probed.preparation }).catch(() => {});
    }
    probed = null;
    fIdentity.hidden = true;
    fSubmit.disabled = true;
  }

  async function loadProviderForm() {
    showError("");
    fFields.replaceChildren();
    invalidate();

    try {
      spec = await invoke("provider_form", { provider: providerId });
    } catch (err) {
      showError(String(err));
      return;
    }

    fGuidance.textContent = spec.guidance;

    if (!spec.tool_ready) {
      showError(`${spec.tool}가 설치되어 있지 않습니다. 도구 상태에서 설치하세요.`);
    }

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
      // 비밀값이 자동완성에 남지 않게.
      input.autocomplete = "off";
      input.spellcheck = false;
      input.addEventListener("input", invalidate);
      wrap.append(input);

      if (field.help) wrap.append(span("field-help", field.help));
      fFields.append(wrap);
    }

    fBrowser.hidden = !spec.browser_url;
    if (spec.browser_url) fBrowser.textContent = spec.browser_label;

    // 받아 적을 값이 없는 provider 는 로그인이 곧 확인이다.
    // 입력칸 수로 판단하면 브라우저 로그인 provider 가 막힌다.
    fProbe.textContent = spec.flow !== "credential" ? "브라우저로 로그인" : "자격 확인";
    fProbe.disabled = spec.flow === "credential" && spec.fields.length === 0;
    challenge.reset();
    fFields.querySelector("input")?.focus();
  }

  // root 상태. AWS 가 만들지 말라고 권고하는 것들이라 문제일 때만 눈에 띄게 한다.
  function rootFacts(result) {
    const rows = [];
    if (result.root_keys_present !== null && result.root_keys_present !== undefined) {
      rows.push([
        "root 키",
        result.root_keys_present ? "있음 — 삭제를 권고합니다" : "없음",
        result.root_keys_present ? "warn" : "muted",
      ]);
    }
    if (result.root_mfa !== null && result.root_mfa !== undefined) {
      rows.push([
        "root MFA",
        result.root_mfa ? "켜짐" : "꺼짐 — 켜는 것을 권고합니다",
        result.root_mfa ? "muted" : "warn",
      ]);
    }
    return rows;
  }

  function fact(label, value, className = "") {
    const row = document.createElement("div");
    row.className = "identity-row";
    row.append(span("identity-label", label));
    row.append(span(`identity-value ${className}`.trim(), value));
    return row;
  }

  function showIdentity(result) {
    fIdentity.replaceChildren();
    fIdentity.hidden = false;

    fIdentity.append(span("identity-name", result.name));
    fIdentity.append(fact("계정 이름", result.slug, "mono"));
    if (result.aws_account_id) {
      fIdentity.append(fact("AWS 계정", result.aws_account_id, "mono"));
    }
    for (const row of rootFacts(result)) fIdentity.append(fact(...row));
    fIdentity.append(
      fact(
        "자격 만료",
        result.expires === "never" ? "기한 없음" : (result.expires ?? "확인 못 함"),
        result.expires === "never" ? "muted" : "",
      ),
    );
    if (result.scopes.length) {
      fIdentity.append(fact("scope", result.scopes.join(", "), "mono wrap"));
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

      probed = spec.flow !== "credential"
        ? await invoke("probe_browser", { provider: providerId })
        : await invoke("probe_credentials", {
            provider: providerId,
            values: collectValues(),
          });

      showIdentity(probed);
      if (!fDisplay.value.trim()) fDisplay.value = probed.display;
      fSubmit.disabled = false;
      fDisplay.focus();
    } catch (err) {
      invalidate();
      showError(String(err));
    } finally {
      fProbe.disabled = false;
      fProbe.textContent = spec.flow !== "credential" ? "브라우저로 로그인" : "자격 확인";
    }
  }

  whenLeaving(invalidate);
  fProbe.addEventListener("click", probe);
  fCancel.addEventListener("click", () => select(null));

  fBrowser.addEventListener("click", () => {
    if (spec?.browser_url) {
      invoke("open_url", { url: spec.browser_url }).catch((err) => showError(String(err)));
    }
  });

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!probed) return;
    showError("");

    fSubmit.disabled = true;
    try {
      // 신원·권한·만료일은 되돌려 보내지 않는다. 확인 단계가 남긴 자격을
      // 가리키는 표만 보내고, 사실은 그쪽에서 온다.
      await invoke("create_account", {
        account: {
          preparation: probed.preparation,
          slug: probed.slug,
          display: fDisplay.value.trim(),
          note: "",
        },
      });
      // 입력한 비밀값을 DOM 에 남기지 않는다. 새 계정은 이벤트로 다시 읽힌다.
      select({ kind: "account", ref: `${providerId}/${probed.slug}` });
    } catch (err) {
      showError(String(err));
      fSubmit.disabled = false;
    }
  });

  loadProviderForm();
}


