// 화면의 조립 지점. 무엇을 언제 읽을지는 여기서만 정한다.

import { loadAccounts } from "./accounts.js";
import { showTab } from "./tabs.js";
import { loadTools } from "./tools.js";
import { onJobFinished } from "./terminal.js";

// 설치 job 이 끝나면 실제 상태를 다시 읽는다. 설치됐다고 가정하지 않는다.
onJobFinished(loadTools);

document.getElementById("refresh").addEventListener("click", loadTools);
loadTools();

showTab("env");

// 만료 알림은 계정 탭을 열지 않아도 보여야 한다.
loadAccounts();
