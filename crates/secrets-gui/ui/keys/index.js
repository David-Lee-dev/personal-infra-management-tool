// 키 화면의 조립 지점 — 도메인 탭을 잡고, 무엇을 보여 줄지 고른다.

import { placeholder } from "../dom.js";
import { termWrite } from "../terminal.js";
import * as aws from "./aws/detail.js";
import { renderList as renderAwsList } from "./aws/list.js";
import { renderRegister } from "./aws/form.js";
import { renderNewAccount } from "./aws/account-form.js";
import { renderIam } from "./aws/iam.js";
import { renderIamRegister } from "./aws/iam-form.js";
import { accountDetail } from "./aws/accounts.js";
import {
  accountsFor,
  awsMaster,
  hostAccountOf,
  iamOf,
  keyOf as awsKeyOf,
  setAwsMaster,
  setHostAccounts,
  setIamUsers,
  setKnown as setAwsKnown,
} from "./aws/state.js";
import { renderItem } from "./etc/detail.js";
import { etcOf, renderList as renderEtcList, setEtcItems } from "./etc/list.js";
import { renderKey, renderUnowned } from "./github/detail.js";
import { renderNewDeploy } from "./github/form.js";
import { renderList } from "./github/list.js";
import {
  DOMAINS,
  domainId,
  keyOf,
  known,
  onChange,
  orphanOf,
  select,
  selected,
  setAccounts,
  setDomain,
  setKnown,
  setUnowned,
  unowned,
} from "./state.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const tabs = document.getElementById("key-domains");
const newKey = document.getElementById("key-new");
const scan = document.getElementById("key-scan");
const mount = document.getElementById("key-body");

function render() {
  for (const button of tabs.querySelectorAll("button[data-domain]")) {
    button.setAttribute("aria-selected", String(button.dataset.domain === domainId()));
  }

  const domain = DOMAINS.find((d) => d.id === domainId());
  const ready = domain?.ready;
  newKey.hidden = !ready || !domain.make;
  scan.hidden = !ready || !domain.scan;
  if (ready) {
    if (domain.make) newKey.textContent = domain.make;
    if (domain.scan) scan.textContent = domain.scan;
  }
  if (!ready) {
    return mount.replaceChildren(placeholder("아직 없습니다", "GitHub 부터 만들고 있습니다."));
  }
  if (domainId() === "aws") return renderAws();
  if (domainId() === "etc") return renderEtc();

  const here = selected();
  if (here?.kind === "new") return renderNewDeploy(mount);

  if (here?.kind === "key") {
    const key = keyOf(here.ref);
    if (key) return renderKey(mount, key);
  }
  if (here?.kind === "unowned") {
    const orphan = orphanOf(here.ref);
    if (orphan) return renderUnowned(mount, orphan);
  }

  if (!known().length && !unowned().length) {
    return mount.replaceChildren(placeholder("키가 없습니다", "＋ 를 눌러 만드세요."));
  }
  renderList(mount);
}

export async function loadKeys() {
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
    (here?.kind === "key" && !keyOf(here.ref)) ||
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

function renderAws() {
  const here = selected();
  if (here?.kind === "aws-new") return renderRegister(mount, awsMaster()?.slug ?? null);
  if (here?.kind === "iam-new") return renderIamRegister(mount, awsMaster());

  if (here?.kind === "iam") {
    const user = iamOf(here.ref);
    if (user) return renderIam(mount, user);
  }

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
  renderAwsList(mount);
}

tabs.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-domain]");
  if (button && !button.disabled) setDomain(button.dataset.domain);
});

const NEW_KIND = { github: "new", aws: "aws-new" };

newKey.addEventListener("click", () => select({ kind: NEW_KIND[domainId()] }));
// 두 번째 버튼. GitHub 에서는 원격 조회, AWS 에서는 IAM 만들기다.
scan.addEventListener("click", () =>
  domainId() === "aws" ? select({ kind: "iam-new" }) : scanUnowned(),
);

listen("keys:updated", loadKeys);

onChange(render);
