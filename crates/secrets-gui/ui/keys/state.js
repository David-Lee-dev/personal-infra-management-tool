// 키 화면이 공유하는 상태와, 무엇을 보고 있는가.
//
// 다시 그리는 일은 index.js 가 한다. 여기서는 무엇이 바뀌었는지만 알린다.

// 안쪽 탭. 버튼은 탭마다 다르다 — 목록 위 오른쪽에 놓인다.
export const DOMAINS = [
  { id: "github", label: "GitHub 배포 키" },
  { id: "iam", label: "AWS IAM" },
  { id: "pem", label: "서버 키" },
  { id: "etc", label: "기타" },
];

const STATE_LABEL = {
  registered: "등록됨",
  local: "미등록",
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

/// 모든 배포 키. 탭 이름 옆 개수처럼 지금 탭과 상관없이 셀 때 쓴다.
export function allKeys() {
  return keys;
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

// 목록에서 찾기 · 거르기. 탭마다 따로 기억하지 않는다 — 탭을 바꾸면 비운다.
let query = "";
let filter = "all";

export function listQuery() {
  return query;
}

export function listFilter() {
  return filter;
}

export function setListQuery(next) {
  query = next;
  redraw();
}

export function setListFilter(next) {
  filter = next;
  redraw();
}

// 도메인을 바꾸면 보던 것은 그 도메인 것이 아니다. 같이 비운다.
export function setDomain(next) {
  domain = next;
  selection = null;
  query = "";
  filter = "all";
  redraw();
}

export function selected() {
  return selection;
}

export function select(next) {
  selection = next;
  redraw();
}
