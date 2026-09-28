// GitHub 배포 키 목록 — 레포 하나가 한 줄이다.
//
// 키는 레포마다 용도별로 여럿이다(develop · deploy). 키마다 한 줄이면 같은 레포 이름이
// 되풀이되어 무엇이 몇 개인지 읽히지 않는다. 한 줄에 그 레포의 키들을 칩으로 두고,
// 쓰는 프로젝트를 옆에 둔다.

import { span } from "../../dom.js";
import { chip, expiryText, group, line, matches, nothing, toolbar } from "../kit.js";
import { known, listFilter, select, stateText, unowned } from "../state.js";
import { githubId, useChips, usesOf } from "../usage.js";

/// 용도 칩 — 권한을 화살표로(↓ 받기, ↑ 밀기). 등록이 끝나지 않았으면 주의.
function keyChip(key) {
  const base = `${key.purpose} ${key.write ? "↓↑" : "↓"}`;
  const text = key.state === "registered" ? base : `${base} · ${stateText(key)}`;
  const expiring = key.expiry.state === "soon" || key.expiry.state === "expired";
  const c = chip(expiring ? `${text} · ${expiryText(key.expiry)}` : text, key.state === "registered" && !expiring ? "" : "warn");
  c.title = `${key.purpose} — ${key.write ? "읽기 · 쓰기" : "읽기 전용"} · ${stateText(key)}`;
  return c;
}

/// 레포마다 키를 모은다.
export function repos() {
  const byRepo = new Map();
  for (const key of known()) {
    if (!byRepo.has(key.repo)) byRepo.set(key.repo, []);
    byRepo.get(key.repo).push(key);
  }
  return [...byRepo.entries()]
    .map(([repo, keys]) => ({
      repo,
      owner: repo.split("/")[0],
      name: repo.split("/").slice(1).join("/"),
      keys: keys.sort((a, b) => a.purpose.localeCompare(b.purpose)),
      uses: usesOf(githubId(repo)),
      attention: keys.some((k) => k.state !== "registered"),
    }))
    .sort((a, b) => a.repo.localeCompare(b.repo));
}

const FILTERS = {
  all: () => true,
  used: (r) => r.uses.length > 0,
  unused: (r) => r.uses.length === 0,
  attention: (r) => r.attention,
};

export function renderList(mount) {
  const all = repos();
  const bar = toolbar({
    placeholder: "레포 · 용도로 찾기",
    filters: [
      { id: "all", label: "전체", count: all.length },
      { id: "used", label: "프로젝트에서 씀", count: all.filter(FILTERS.used).length },
      { id: "unused", label: "쓰는 곳 없음", count: all.filter(FILTERS.unused).length },
      { id: "attention", label: "손볼 것", count: all.filter(FILTERS.attention).length, tone: "warn" },
    ],
  });

  const shown = all.filter(FILTERS[listFilter()] ?? FILTERS.all).filter((r) =>
    matches(r.repo, ...r.keys.map((k) => k.purpose)),
  );
  const parts = [bar];
  if (!shown.length) parts.push(nothing(all.length ? "찾는 레포가 없습니다." : "배포 키가 없습니다. ＋ 배포 키 만들기로 시작하세요."));

  for (const owner of [...new Set(shown.map((r) => r.owner))]) {
    const rows = shown
      .filter((r) => r.owner === owner)
      .map((r) =>
        line({
          title: r.name,
          chips: r.keys.map(keyChip),
          uses: useChips(r.uses),
          tone: r.attention ? "warn" : "",
          onClick: () => select({ kind: "repo", ref: r.repo }),
        }),
      );
    parts.push(group(owner, rows));
  }

  // 원격을 아직 묻지 않았으면 없다고 말하지 않는다. 모르는 것과 없는 것은 다르다.
  const orphans = unowned().filter((o) => matches(o.title, o.repo, o.account));
  if (orphans.length) {
    parts.push(
      group(
        "GitHub에만 있는 키",
        orphans.map((o) =>
          line({
            title: o.title,
            sub: `${o.account} · ${o.repo ?? "계정 전체"}`,
            chips: [chip("개인 키 없음", "warn")],
            uses: span("mono small muted", o.fingerprint),
            tone: "warn",
            onClick: () => select({ kind: "unowned", ref: o.ref }),
          }),
        ),
        { note: "이 금고에 개인 키가 없는 GitHub 등록" },
      ),
    );
  }
  mount.replaceChildren(...parts);
}
