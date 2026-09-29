// 코드를 되돌려 넣어야 끝나는 브라우저 로그인 (firebase).

import { invoke } from "../ipc.js";

/* ── 코드 입력이 필요한 브라우저 로그인 ─────────────── */

/// 폼 안의 코드 입력 단계를 묶어 다룬다. 계정 추가와 재발급이 함께 쓴다.
export function bindChallenge(form, providerId, { onDone, onError }) {
  const box = form.querySelector("#f-challenge");
  const fCode = form.querySelector("#f-code");
  const fOpen = form.querySelector("#f-open-auth");
  const fDone = form.querySelector("#f-code-submit");
  const fRestart = form.querySelector("#f-restart");
  const fSession = form.querySelector("#f-session");
  const fProbe = form.querySelector("#f-probe");

  let authUrl = null;
  // 두 단계가 같은 로그인을 가리키게 하는 표.
  let preparation = null;

  function reset() {
    box.hidden = true;
    fCode.value = "";
    authUrl = null;
    fProbe.disabled = false;
  }

  // 로그인을 시작해 인증 주소를 받아 연다.
  //
  // 다시 시작하면 CLI 가 세션을 새로 만들어 앞서 받은 코드가 무효해진다.
  // 그래서 진행 중에는 시작 버튼을 막고, 다시 시작은 따로 누르게 한다.
  async function begin() {
    const challenge = await invoke("begin_browser_login", { provider: providerId });
    preparation = challenge.preparation;
    authUrl = challenge.url;
    fSession.textContent = challenge.session || "—";
    box.hidden = false;
    fProbe.disabled = true;
    fCode.value = "";
    fCode.focus();
    // 주소를 받자마자 열어 준다. 실패해도 버튼으로 다시 열 수 있다.
    invoke("open_url", { url: authUrl }).catch(() => {});
  }

  fRestart.addEventListener("click", async () => {
    onError("");
    try {
      await begin();
    } catch (err) {
      onError(String(err));
      reset();
    }
  });

  fOpen.addEventListener("click", () => {
    if (authUrl) invoke("open_url", { url: authUrl }).catch((err) => onError(String(err)));
  });

  fDone.addEventListener("click", async () => {
    onError("");
    fDone.disabled = true;
    fDone.textContent = "확인 중…";

    try {
      const result = await invoke("complete_browser_login", {
        preparation,
        code: fCode.value,
      });
      reset();
      onDone(result);
    } catch (err) {
      // 한 번 실패하면 CLI 가 세션을 버린다. 같은 칸에 다시 넣어 봐야 같은
      // 오류만 나오므로, 여기서 새 세션을 받아 코드부터 다시 받게 한다.
      try {
        await begin();
        onError(`${err} 새 인증 페이지를 열었습니다. 새 코드를 받아 입력하세요.`);
      } catch (restartErr) {
        reset();
        onError(`${err} (다시 시작하지 못했습니다: ${restartErr})`);
      }
    } finally {
      fDone.disabled = false;
      fDone.textContent = "코드로 완료";
    }
  });

  return { begin, reset };
}
