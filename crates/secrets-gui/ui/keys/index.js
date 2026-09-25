// 키 화면의 조립 지점 — 도메인 탭을 잡고, 무엇을 보여 줄지 고른다.

import { termWrite } from "../terminal.js";
import * as aws from "./aws/detail.js";
import { renderList as renderPemList } from "./aws/list.js";
import { renderRegister } from "./aws/form.js";
import { renderNewAccount } from "./aws/account-form.js";
import { renderIam, renderIamList } from "./aws/iam.js";
import { renderIamRegister } from "./aws/iam-form.js";
import { renderIamAdopt } from "./aws/iam-adopt.js";
import { accountDetail } from "./aws/accounts.js";
import {
  accountsFor,
  awsMaster,
  hostAccountOf,
  iamOf,
  iamUsers,
  keyOf as awsKeyOf,
  known as pemKeys,
  setAwsMaster,
  setHostAccounts,
  setIamUsers,
  setKnown as setAwsKnown,
} from "./aws/state.js";
import { renderItem } from "./etc/detail.js";
import { etcItems, etcOf, renderList as renderEtcList, setEtcItems } from "./etc/list.js";
import { renderRepo, renderUnowned } from "./github/detail.js";
import { renderNewDeploy } from "./github/form.js";
import { renderList } from "./github/list.js";
import { loadUsage } from "./usage.js";
import {
  DOMAINS,
  allKeys,
  domainId,
  known,
  onChange,
  orphanOf,
  select,
  selected,
  setAccounts,
  setDomain,
  setKnown,
  setUnowned,
} from "./state.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const tabs = document.getElementById("key-domains");
const actions = document.getElementById("key-actions");
const mount = document.getElementById("key-body");

/// 탭마다 목록 위 오른쪽의 버튼. 두 번째 것이 주 버튼이다.
function domainActions() {
  switch (domainId()) {
    case "github":
      return [
        ["GitHub에서 조회", () => scanUnowned()],
        ["＋ 배포 키 만들기", () => select({ kind: "new" })],
      ];
    case "iam":
      return [
        ["기존 IAM 들이기", () => select({ kind: "iam-adopt" })],
        ["＋ IAM 만들기", () => select({ kind: "iam-new" })],
      ];
    case "pem":
      return [["＋ pem 키 등록", () => select({ kind: "aws-new" })]];
    default:
      return [];
  }
}

/// 탭 이름 옆의 개수.
function counts() {
  const repos = new Set(allKeys().filter((k) => k.domain === "github").map((k) => k.repo)).size;
  return { github: repos, iam: iamUsers().length, pem: pemKeys().length, etc: etcItems().length };
}

function render() {
  const n = counts();
  for (const button of tabs.querySelectorAll("button[data-domain]")) {
    const domain = DOMAINS.find((d) => d.id === button.dataset.domain);
    button.setAttribute("aria-selected", String(button.dataset.domain === domainId()));
    button.replaceChildren(domain.label, " ");
    const count = document.createElement("span");
    count.className = "tab-count";
    count.textContent = String(n[domain.id] ?? 0);
    button.append(count);
  }

  // 버튼은 목록을 볼 때만. 상세 · 만들기 화면에서는 그 화면의 버튼만 있다.
  const listing = !selected();
  actions.replaceChildren(
    ...(listing
      ? domainActions().map(([label, onClick], index, all) => {
          const b = document.createElement("button");
          b.type = "button";
          b.textContent = label;
          if (index === all.length - 1) b.className = "primary";
          b.addEventListener("click", onClick);
          return b;
        })
      : []),
  );

  if (domainId() === "iam") return renderIamDomain();
  if (domainId() === "pem") return renderPem();
  if (domainId() === "etc") return renderEtc();

  const here = selected();
  if (here?.kind === "new") return renderNewDeploy(mount);
  if (here?.kind === "repo" && known().some((k) => k.repo === here.ref)) return renderRepo(mount, here.ref);
  if (here?.kind === "unowned") {
    const orphan = orphanOf(here.ref);
    if (orphan) return renderUnowned(mount, orphan);
  }
  renderList(mount);
}

export async function loadKeys() {
  await loadUsage();
  try {
    const held = await invoke("list_aws_keys");
    setAwsKnown(held.keys);
    for (const message of held.errors) termWrite("err", message);

    // 계정은 pem 아래에 산다. pem 을 읽은 뒤 그 계정들을 모은다.
    const seen = new Set();
    const boxes = [];
    for (const key of held.keys) {
      const at = `${key.account}/${key.machine}`;
      if (seen.has(at)) continue;
      seen.add(at);
      const mine = await invoke("list_instance_accounts", {
        awsAccount: key.account,
        machine: key.machine,
      });
      boxes.push(...mine.accounts);
      for (const message of mine.errors) termWrite("err", message);
    }
    setHostAccounts(boxes);
  } catch (err) {
    setAwsKnown([]);
    setHostAccounts([]);
    termWrite("err", `AWS 키 목록을 읽지 못했습니다: ${err}`);
  }

  try {
    const held = await invoke("list_iam");
    setIamUsers(held.users);
    for (const message of held.errors) termWrite("err", message);
  } catch (err) {
    setIamUsers([]);
    termWrite("err", `IAM 목록을 읽지 못했습니다: ${err}`);
  }

  try {
    const result = await invoke("list_keys");
    setKnown(result.keys);
    // 읽지 못한 기록을 조용히 숨기면 키가 사라진 것처럼 보인다.
    for (const message of result.errors) termWrite("err", message);
  } catch (err) {
    setKnown([]);
    termWrite("err", `키 목록을 읽지 못했습니다: ${err}`);
  }

  try {
    const held = await invoke("list_etc");
    setEtcItems(held.items);
    for (const message of held.errors) termWrite("err", message);
  } catch (err) {
    setEtcItems([]);
    termWrite("err", `기타 목록을 읽지 못했습니다: ${err}`);
  }

  // 계정 목록은 만들기 폼이 쓴다. 키 화면이 계정을 따로 알 이유는 없다.
  try {
    const accounts = await invoke("list_accounts");
    setAccounts(
      accounts.accounts.filter((a) => a.provider === "github").map((a) => a.slug),
    );
    const aws = accounts.accounts.find((a) => a.provider === "aws");
    setAwsMaster(aws ? { slug: aws.slug, account: aws.aws_account_id } : null);
  } catch {
    setAccounts([]);
  }

  // 보던 것이 사라졌으면 목록으로 돌아간다.
  const here = selected();
  if (
    (here?.kind === "repo" && !known().some((k) => k.repo === here.ref)) ||
    (here?.kind === "unowned" && !orphanOf(here.ref)) ||
    (here?.kind === "iam" && !iamOf(here.ref)) ||
    (here?.kind === "etc" && !etcOf(here.ref))
  ) {
    return select(null);
  }
  render();
}

// 원격을 실제로 묻는 유일한 자리. 목록은 로컬만 읽어 빠르게 뜬다.
async function scanUnowned() {
  const accounts = await invoke("list_accounts").catch(() => ({ accounts: [] }));
  const github = accounts.accounts.filter((a) => a.provider === "github");

  const found = [];
  for (const account of github) {
    try {
      found.push(...(await invoke("scan_unowned", { account: account.slug })));
    } catch (err) {
      termWrite("err", `${account.slug}: ${err}`);
    }
  }
  setUnowned(found);
  render();
}

function renderEtc() {
  const here = selected();
  if (here?.kind === "etc") {
    const item = etcOf(here.ref);
    if (item) return renderItem(mount, item);
  }
  renderEtcList(mount);
}

function renderIamDomain() {
  const here = selected();
  if (here?.kind === "iam-new") return renderIamRegister(mount, awsMaster());
  if (here?.kind === "iam-adopt") return renderIamAdopt(mount, awsMaster());
  if (here?.kind === "iam") {
    const user = iamOf(here.ref);
    if (user) return renderIam(mount, user);
  }
  renderIamList(mount);
}

function renderPem() {
  const here = selected();
  if (here?.kind === "aws-new") return renderRegister(mount, awsMaster()?.slug ?? null);
  if (here?.kind === "host-new") {
    const key = awsKeyOf(here.ref);
    if (key) {
      return renderNewAccount(mount, key, {
        onBack: () => select({ kind: "aws-key", ref: key.ref }),
        onChanged: () => {
          select({ kind: "aws-key", ref: key.ref });
          loadKeys();
        },
      });
    }
  }

  if (here?.kind === "host-account") {
    const box = hostAccountOf(here.ref);
    const key = box && awsKeyOf(here.key);
    if (box && key) {
      const back = () => select({ kind: "aws-key", ref: key.ref });
      return mount.replaceChildren(
        ...accountDetail(key, box, { onBack: back, onChanged: () => { back(); loadKeys(); } }),
      );
    }
  }

  if (here?.kind === "aws-key") {
    const key = awsKeyOf(here.ref);
    if (key) {
      return aws.renderKey(mount, key, accountsFor(key), {
        onCreate: () => select({ kind: "host-new", ref: key.ref }),
        onOpen: (box) => select({ kind: "host-account", ref: box.ref, key: key.ref }),
      });
    }
  }
  renderPemList(mount);
}

tabs.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-domain]");
  if (button && !button.disabled) setDomain(button.dataset.domain);
});

listen("keys:updated", loadKeys);

onChange(render);
