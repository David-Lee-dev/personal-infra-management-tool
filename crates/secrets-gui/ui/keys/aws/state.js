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

// 등록된 서버. pem 마다 그 pem 으로 들어가는 계정이 있는 서버를 찾는 데 쓴다.
let servers = [];

export function setServers(next) {
  servers = next;
}

/// 이 pem 으로 들어가는 계정이 있는 서버와 그 계정들. `{ server, logins }`.
export function serversFor(key) {
  return servers
    .filter((s) => s.kind === key.machine && s.aws?.account === key.account && s.aws?.region === key.region)
    .map((s) => ({ server: s, logins: s.accounts.filter((a) => a.pem === key.name).map((a) => a.login) }))
    .filter((found) => found.logins.length);
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
