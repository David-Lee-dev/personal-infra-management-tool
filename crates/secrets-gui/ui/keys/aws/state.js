// AWS 화면이 쥐고 있는 것.
//
// 목록은 **이 금고가 들인 것**이다. AWS 에 무엇이 있는지는 등록할 때만 묻는다 —
// 그러지 않으면 이 화면이 AWS 를 그냥 비추는 창이 된다.

let held = [];

export function known() {
  return held;
}

export function setKnown(next) {
  held = next;
}

export function keyOf(ref) {
  return held.find((key) => key.ref === ref);
}

// 이 금고가 들인 인스턴스 계정.
let accounts = [];

export function hostAccounts() {
  return accounts;
}

export function setHostAccounts(next) {
  accounts = next;
}

export function accountsFor(key) {
  return accounts.filter((box) => box.keypair === key.name && box.region === key.region);
}

export function hostAccountOf(ref) {
  return accounts.find((box) => box.ref === ref);
}

// 이 금고가 만든 IAM.
let iam = [];

export function iamUsers() {
  return iam;
}

export function setIamUsers(next) {
  iam = next;
}

export function iamOf(ref) {
  return iam.find((user) => user.ref === ref);
}

// IAM 을 만들 마스터 계정과 그 AWS 계정 ID. 목록을 읽을 때 잡아 둔다.
let master = null;

export function awsMaster() {
  return master;
}

export function setAwsMaster(next) {
  master = next;
}
