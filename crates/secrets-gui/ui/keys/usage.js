// 자격 증명마다 그것을 쓰는 프로젝트 · 환경. 목록과 상세가 "어디에 쓰이나"를 보인다.
//
// 연결은 프로젝트 화면에서 한다. 여기서는 읽기만 한다.

import { span } from "../dom.js";
import { invoke } from "../ipc.js";

let uses = [];

export async function loadUsage() {
  try {
    uses = await invoke("credential_usage");
  } catch {
    uses = [];
  }
}

/// `github:<소유자/레포>` · `iam:<ref>` · `etc:<slug>` · `pem:<리전/키페어>` 를 쓰는 곳.
export function usesOf(credential) {
  return uses.filter((u) => u.credential === credential);
}

export function githubId(repo) {
  return "github:" + repo.toLowerCase();
}

/// 프로젝트마다 한 칩 — 그 프로젝트에서 이것을 쓰는 환경들을 꼬리로 붙인다.
function byProject(list) {
  const seen = new Map();
  for (const u of list) {
    if (!seen.has(u.project)) seen.set(u.project, { project: u.project, envs: [] });
    if (u.environment && !seen.get(u.project).envs.includes(u.environment)) seen.get(u.project).envs.push(u.environment);
  }
  return [...seen.values()];
}

/// 프로젝트 칩 — `tuk-api-server | prod | dev`. 환경이 없으면 프로젝트 이름만.
export function projectChip(u) {
  const chip = span("use-chip", "");
  chip.append(span("use-project", u.project));
  const envs = u.envs ?? (u.environment ? [u.environment] : []);
  for (const env of envs) chip.append(span("use-env", env));
  return chip;
}

/// 목록 줄의 사용처 칸. 프로젝트가 없으면 `extra`(기록된 다른 곳)를 보이고, 그것도 없으면 "쓰는 곳 없음".
export function useChips(list, { extra = [], max = 3 } = {}) {
  const box = span("use-chips", "");
  const shown = byProject(list);
  for (const u of shown.slice(0, max)) box.append(projectChip(u));
  if (shown.length > max) box.append(span("use-more", `+${shown.length - max}`));
  for (const text of extra) box.append(span("use-chip other", text));
  if (!shown.length && !extra.length) box.append(span("use-none", "쓰는 곳 없음"));
  return box;
}

/// 사용 위치 기록 하나가 어느 프로젝트의 파일인가.
export function useOfPlace(uses, consumer) {
  return uses.find((u) => (u.host ?? "local") === consumer.host && u.file && consumer.file.endsWith("/" + u.file));
}

/// 프로젝트에 닿지 않는 사용 위치 기록 — 목록에서 호스트 이름(이 맥이면 "이 맥")으로 보인다.
export function unplaced(consumers, uses) {
  const labels = consumers
    .filter((c) => !useOfPlace(uses, c))
    .map((c) => (c.host === "local" ? "로컬" : c.host));
  return [...new Set(labels)];
}
