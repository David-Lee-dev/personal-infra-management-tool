// 모듈 그래프를 실제로 읽어 import 대상과 named export 를 대조한다.
import { readFileSync, existsSync } from "node:fs";
import { dirname, resolve, relative } from "node:path";

const root = process.argv[2];
const entry = resolve(root, "main.js");
const problems = [];
const seen = new Set();

function exportsOf(source) {
  const names = new Set();
  for (const m of source.matchAll(/export\s+(?:async\s+)?(?:function|const|let|class)\s+([A-Za-z_$][\w$]*)/g)) names.add(m[1]);
  for (const m of source.matchAll(/export\s*\{([^}]*)\}/g)) {
    for (const part of m[1].split(",")) {
      const name = part.trim().split(/\s+as\s+/).pop().trim();
      if (name) names.add(name);
    }
  }
  return names;
}

function walk(file) {
  if (seen.has(file)) return;
  seen.add(file);
  if (!existsSync(file)) { problems.push(`없는 모듈: ${relative(root, file)}`); return; }
  const source = readFileSync(file, "utf8");

  for (const m of source.matchAll(/import\s*(?:\{([^}]*)\}|([A-Za-z_$][\w$]*))?\s*from\s*["']([^"']+)["']/g)) {
    const target = resolve(dirname(file), m[3]);
    if (!existsSync(target)) {
      problems.push(`${relative(root, file)} -> 없는 모듈 ${m[3]}`);
      continue;
    }
    const wanted = (m[1] ?? "").split(",").map((s) => s.trim().split(/\s+as\s+/)[0].trim()).filter(Boolean);
    const provided = exportsOf(readFileSync(target, "utf8"));
    for (const name of wanted) {
      if (!provided.has(name)) problems.push(`${relative(root, file)} -> ${m[3]} 에 export ${name} 이 없다`);
    }
    walk(target);
  }
}

walk(entry);
if (problems.length) { for (const p of problems) console.error(p); process.exit(1); }
