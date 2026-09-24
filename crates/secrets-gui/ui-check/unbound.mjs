// 다른 모듈이 정의한 이름을 import 없이 쓰는 것을 잡는다.
//
// 모듈을 쪼갤 때 참조가 끊겨도 문법은 멀쩡하고 import 경로도 멀쩡해서,
// 브라우저에서 그 줄이 실제로 실행될 때까지 아무도 모른다. 그 구멍만 막는다.
//
// 우리 코드가 어딘가에 정의한 이름에만 반응한다. 브라우저 전역이나 표준 내장은
// 애초에 후보가 아니라, 잘못 짚을 일이 없다.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = process.argv[2];

function sources(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) out.push(...sources(path));
    else if (name.endsWith(".js")) out.push(path);
  }
  return out;
}

// 주석·문자열은 식별자가 아니다. 지우고 본다.
//
// 한 번에, 앞에서부터 지운다. 종류별로 차례로 지우면 문자열 `"dev/*"` 의 `/*` 가
// 주석 시작으로 읽혀 다음 `*/` 까지 코드가 통째로 사라진다.
function bare(source) {
  return source.replace(
    /\/\*[\s\S]*?\*\/|\/\/[^\n]*|`(?:\\.|\$\{[^}]*\}|[^`\\])*`|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/g,
    " ",
  );
}

// 이 파일이 스스로 묶은 이름. 스코프는 보지 않는다 —
// 놓치는 쪽이지 잘못 짚는 쪽이 아니다.
function bound(text) {
  const names = new Set();
  const add = (chunk) => {
    for (const m of chunk.matchAll(/[A-Za-z_$][\w$]*/g)) names.add(m[0]);
  };
  for (const m of text.matchAll(/(?:function|const|let|var|class)\s+([A-Za-z_$][\w$]*)/g)) names.add(m[1]);
  // 구조 분해로 묶이는 이름.
  for (const m of text.matchAll(/(?:const|let|var)\s*([{[][^=;]*[}\]])\s*=/g)) add(m[1]);
  // 함수 · 화살표 함수의 매개변수.
  for (const m of text.matchAll(/(?:function\s*[A-Za-z_$][\w$]*\s*|function\s*)\(([^)]*)\)/g)) add(m[1]);
  for (const m of text.matchAll(/\(([^()]*)\)\s*=>/g)) add(m[1]);
  for (const m of text.matchAll(/(?:^|[^\w$.])([A-Za-z_$][\w$]*)\s*=>/gm)) names.add(m[1]);
  for (const m of text.matchAll(/catch\s*\(([^)]*)\)/g)) add(m[1]);
  for (const m of text.matchAll(/for\s*\(\s*(?:const|let|var)\s+([^;]*?)\s+(?:of|in)\s/g)) add(m[1]);
  // import 로 들어온 이름.
  for (const m of text.matchAll(/import\s*\{([^}]*)\}/g)) {
    for (const part of m[1].split(",")) {
      const name = part.trim().split(/\s+as\s+/).pop().trim();
      if (name) names.add(name);
    }
  }
  for (const m of text.matchAll(/import\s+([A-Za-z_$][\w$]*)\s+from/g)) names.add(m[1]);
  return names;
}

// 값으로 읽히는 이름. 속성 접근과 객체 키는 뺀다.
function used(text) {
  const names = new Set();
  // import 문에 적힌 이름은 쓰임이 아니라 묶임이다.
  text = text.replace(/import[^;]*from[^;]*;/g, " ");
  for (const m of text.matchAll(/(^|[^\w$?.])([A-Za-z_$][\w$]*)(\s*)(:?)/gm)) {
    // `{ kind: … }` 처럼 바로 붙은 콜론은 키다. 삼항의 `a ? b : c` 는 띄어 쓴다.
    if (m[4] === ":" && m[3] === "") continue;
    names.add(m[2]);
  }
  return names;
}

const files = sources(root).filter((f) => !f.includes("node_modules"));
const text = new Map(files.map((f) => [f, bare(readFileSync(f, "utf8"))]));
const declared = new Map([...text].map(([f, t]) => [f, bound(t)]));

// 우리 코드가 정의한 이름 전부. 여기 있는 것만 후보로 본다.
const ours = new Set();
for (const [, t] of text) {
  for (const m of t.matchAll(/(?:^|\n)\s*(?:export\s+)?(?:async\s+)?(?:function|const|let|var|class)\s+([A-Za-z_$][\w$]*)/g)) {
    ours.add(m[1]);
  }
}

const problems = [];
for (const file of files) {
  const mine = declared.get(file);
  for (const name of used(text.get(file))) {
    if (!ours.has(name) || mine.has(name)) continue;
    problems.push(`${relative(root, file)} -> ${name} 은 다른 모듈의 것인데 import 하지 않았다`);
  }
}

if (problems.length) {
  for (const p of problems.sort()) console.error(p);
  process.exit(1);
}
