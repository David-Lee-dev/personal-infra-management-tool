// 계정 화면이 공유하는 상태와, 무엇을 보고 있는가.
//
// 화면을 다시 그리는 일은 index.js 가 한다. 여기서는 무엇이 바뀌었는지만 알린다.



export const PROVIDERS = [
  { id: "github", label: "GitHub" },
  { id: "aws", label: "AWS" },
  { id: "gcloud", label: "Google Cloud" },
  { id: "firebase", label: "Firebase" },
];

export function providerLabelOf(id) {
  return PROVIDERS.find((p) => p.id === id)?.label ?? id;
}

// 지금 화면에 띄운 것. {kind: "account", ref} | {kind: "new", provider} | null

const EXPIRY_LABEL = {
  expired: (d) => (d === 0 ? "오늘 만료" : `${d}일 전 만료됨`),
  soon: (d) => (d === 0 ? "오늘 만료" : `${d}일 남음`),
  ok: () => "유효",
  never: () => "기한 없음",
  unset: () => "확인 안 됨",
};

export function expiryText(acc) {
  return (EXPIRY_LABEL[acc.expiry] ?? (() => acc.expiry))(acc.expiry_days ?? 0);
}

export function refOf(acc) {
  return `${acc.provider}/${acc.slug}`;
}

// 목록과 지금 보고 있는 것.
let accounts = [];
let selection = null;

// 화면을 떠나기 전에 반드시 불린다. 확인만 해 둔 자격이 준비 홈에 남지 않게 한다.
let leaving = null;

// 상태가 바뀌면 다시 그릴 사람.
let redraw = () => {};

export function onChange(fn) {
  redraw = fn;
}

export function known() {
  return accounts;
}

export function setKnown(next) {
  accounts = next;
}

export function selected() {
  return selection;
}

export function accountOf(ref) {
  return accounts.find((a) => refOf(a) === ref);
}

export function whenLeaving(discard) {
  leaving = discard;
}

export function select(next) {
  if (leaving) {
    const discard = leaving;
    leaving = null;
    discard();
  }
  selection = next;
  redraw();
}
