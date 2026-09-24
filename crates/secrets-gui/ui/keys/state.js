// 키 화면이 공유하는 상태와, 무엇을 보고 있는가.
//
// 다시 그리는 일은 index.js 가 한다. 여기서는 무엇이 바뀌었는지만 알린다.

// 안쪽 탭. github 만 구현돼 있고 나머지는 자리만 보여 준다.
export const DOMAINS = [
  { id: "github", label: "GitHub", ready: true, make: "＋ 배포 키 만들기", scan: "GitHub 에서 조회" },
  { id: "aws", label: "AWS", ready: true, make: "＋ pem 키 등록", scan: "＋ IAM 만들기" },
  { id: "etc", label: "기타", ready: true, make: null, scan: null },
];

const STATE_LABEL = {
  registered: "등록됨",
  local: "등록 안 됨",
  rotating: "재발급 중단됨",
};

export function stateText(key) {
  return STATE_LABEL[key.state] ?? key.state;
}

export function stateTone(key) {
  return key.state === "registered" ? "" : "warn";
}

export function permissionText(key) {
  return key.write ? "쓰기 허용" : "읽기 전용";
}

let keys = [];
let orphans = [];
let accounts = [];
let domain = "github";
let selection = null;
let redraw = () => {};

export function onChange(fn) {
  redraw = fn;
}

export function known() {
  return keys.filter((k) => k.domain === domain);
}

export function unowned() {
  return orphans.filter((o) => o.domain === domain);
}

export function accountsOf() {
  return accounts;
}

export function setKnown(next) {
  keys = next;
}

export function setUnowned(next) {
  orphans = next;
}

export function setAccounts(next) {
  accounts = next;
}

export function keyOf(ref) {
  return keys.find((k) => k.ref === ref);
}

// 원격을 실제로 물어본 적이 있는가. 아직이면 "없음"이 아니라 "모름"이다.
let scanned = false;

export function hasScanned() {
  return scanned;
}

export function markScanned() {
  scanned = true;
}

export function orphanOf(ref) {
  return orphans.find((o) => o.ref === ref);
}

export function domainId() {
  return domain;
}

// 도메인을 바꾸면 보던 것은 그 도메인 것이 아니다. 같이 비운다.
export function setDomain(next) {
  domain = next;
  selection = null;
  redraw();
}

export function selected() {
  return selection;
}

export function select(next) {
  selection = next;
  redraw();
}
