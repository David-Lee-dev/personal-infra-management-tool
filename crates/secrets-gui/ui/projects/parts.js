// 프로젝트 화면들이 함께 쓰는 조각 — 단계 표시와 다음 할 일.

import { span } from "../dom.js";

const STAGE_TEXT = { done: "연결됨", warn: "주의", pending: "아직" };

/// 로컬 · Git · 서버 세 칸. 모양과 글자로 상태를 함께 보여 준다.
export function stageTrack(stages) {
  const box = document.createElement("span");
  box.className = "stage-track";
  for (const [key, label] of [["local", "로컬"], ["git", "Git"], ["server", "서버"]]) {
    const pill = span(`stage-pill ${stages[key]}`, label);
    pill.title = `${label} ${STAGE_TEXT[stages[key]]}`;
    box.append(pill);
  }
  return box;
}

/// 다음에 할 일 한 줄과 그 무게. 주의가 단계보다 먼저다.
export function nextStep(project) {
  const scan = project.scan;
  if (scan.error) return { text: "디렉토리를 찾을 수 없음", tone: "warn" };
  const exposed = scan.env_files.filter((f) => f.exposed);
  if (exposed.length) return { text: `${exposed[0].name}을(를) .gitignore에 추가`, tone: "warn" };
  if (project.stages.git !== "done") return { text: "Git 연결", tone: "step" };
  if (project.stages.server !== "done") return { text: "서버 연결", tone: "step" };
  return { text: "—", tone: "none" };
}

/// git 상태 한 줄.
export function gitLine(git) {
  if (!git) return "—";
  if (git.kind === "absent") return "git 없음";
  const where = git.kind === "remote" ? git.repo ?? git.origin : "로컬 git만 있음";
  const branch = git.branch ? ` · ${git.branch}` : "";
  const changes = git.changes ? ` · 변경 ${git.changes}` : "";
  return `${where}${branch}${changes}`;
}

/// 런타임 한 줄. 버전이 맞지 않는 것은 표시한다.
export function runtimeLine(runtimes) {
  if (!runtimes.length) return "감지하지 못함";
  return runtimes
    .map((r) => (r.conflict ? `${r.label} (버전 불일치)` : r.version ? `${r.label} ${r.version}` : r.label))
    .join(" · ");
}
