// 백엔드를 부르는 유일한 통로. 실패하면 어느 작업이 왜 실패했는지 작업 로그에 남긴다.
//
// 화면마다 실패를 제 자리에 보여 주기도 하지만, 그것만으로는 지나간 실패를 다시 볼 수 없고
// 조용히 삼키는 곳도 생긴다. 여기서 한 번에 남기면 어떤 화면에서 났든 로그에 원인이 있다.
// 인자는 적지 않는다 — 비밀값이 들어갈 수 있다.

import { setTermStatus, termWrite } from "./terminal.js";

const tauri = window.__TAURI__.core;

/**
 * `quiet` 는 입력하는 동안 되풀이해 묻는 조회에만 쓴다. 타자마다 실패가 로그를 채우면
 * 정작 사람이 누른 작업의 실패가 묻힌다. 그런 조회의 결과는 입력칸 옆에 바로 보인다.
 */
export function invoke(command, args, { quiet = false } = {}) {
  return tauri.invoke(command, args).catch((err) => {
    if (!quiet) {
      const cause = String(err?.message ?? err).trim() || "원인이 전달되지 않았습니다.";
      termWrite("err", `✗ ${command} 실패 — ${cause}`);
      setTermStatus(`실패: ${command} — 작업 로그에서 원인을 보세요`, "fail");
    }
    throw err;
  });
}
