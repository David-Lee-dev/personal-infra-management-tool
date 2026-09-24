// 왼쪽 메뉴의 프로젝트 목록. 그룹별로, 이름 옆에 단계 점 세 개.

import { span } from "../dom.js";

const STAGE_TEXT = { done: "연결됨", warn: "주의", pending: "없음" };

function pips(stages) {
  const box = document.createElement("span");
  box.className = "pips";
  const labels = [["local", "로컬"], ["git", "Git"], ["server", "서버"]];
  box.setAttribute(
    "aria-label",
    labels.map(([key, label]) => `${label} ${STAGE_TEXT[stages[key]]}`).join(" · "),
  );
  for (const [key] of labels) box.append(span(`pip ${stages[key]}`, ""));
  return box;
}

export function renderSide(mount, projects, current, onOpen) {
  const nodes = [];
  const groups = [...new Set(projects.map((p) => p.group))];
  for (const group of groups) {
    nodes.push(span("side-group", group));
    for (const project of projects.filter((p) => p.group === group)) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "side-project";
      if (project.name === current) button.setAttribute("aria-current", "page");
      button.append(span("side-project-name", project.name), pips(project.stages));
      button.addEventListener("click", () => onOpen(project.name));
      nodes.push(button);
    }
  }
  mount.replaceChildren(...nodes);
}
