// Git 연결 — 원격 레포와 이 레포 전용 SSH 키를 로컬 레포에 잇는다.
//
// 이미 origin 이 있으면 그 레포를 그대로 쓰고, 시크릿 저장소에 그 레포의 키가 있으면
// 그 키를 쓴다. 없는 것만 새로 만든다. 이 창에서는 키를 지우지 않는다.

import { span } from "../dom.js";
import { modal } from "../modal.js";

const { invoke } = window.__TAURI__.core;

function section(title, ...children) {
  const box = document.createElement("section");
  box.className = "git-section";
  const h = document.createElement("h3");
  h.textContent = title;
  box.append(h, ...children);
  return box;
}

function radio(group, value, checked, label, note) {
  const row = document.createElement("label");
  row.className = "choice-row";
  const input = document.createElement("input");
  input.type = "radio";
  input.name = group;
  input.value = value;
  input.checked = checked;
  const text = document.createElement("span");
  text.className = "choice-text";
  text.append(span("strong", label));
  if (note) text.append(span("muted small", note));
  row.append(input, text);
  return { row, input };
}

function textInput(value = "", placeholder = "") {
  const input = document.createElement("input");
  input.type = "text";
  input.value = value;
  input.placeholder = placeholder;
  input.spellcheck = false;
  input.autocomplete = "off";
  input.className = "mono";
  return input;
}

function labeled(label, input) {
  const box = document.createElement("div");
  box.className = "field";
  const el = document.createElement("label");
  el.textContent = label;
  box.append(el, input);
  return box;
}

/* ── 점검 ───────────────────────────────────────────── */

/// 연결을 막는 것. 비어 있으면 통과다.
function checks(plan, project, reload) {
  const nodes = [];
  if (plan.tracked.length) {
    nodes.push(
      span(
        "notice warn",
        `${plan.tracked.join(", ")}은(는) git이 이미 추적하고 있어 .gitignore로는 막을 수 없습니다. ` +
          "터미널에서 git rm --cached <파일>로 추적을 멈춘 뒤 다시 여세요.",
      ),
    );
  }
  if (plan.ignorable.length) {
    const line = document.createElement("div");
    line.className = "notice warn with-action";
    const fix = document.createElement("button");
    fix.type = "button";
    fix.textContent = ".gitignore에 추가";
    fix.addEventListener("click", async () => {
      fix.disabled = true;
      try {
        await invoke("ignore_env_files", { name: project.name });
        reload();
      } catch (err) {
        fix.disabled = false;
        line.append(span("problem", String(err)));
      }
    });
    line.append(span("", `${plan.ignorable.join(", ")}의 값이 원격 레포에 올라갈 수 있습니다.`), fix);
    nodes.push(line);
  }
  if (!plan.accounts.length) {
    nodes.push(span("notice warn", "GitHub 계정이 없습니다. 인프라 › 계정에서 GitHub 계정을 먼저 연결하세요."));
  }
  if (plan.git === "remote" && !plan.repo) {
    nodes.push(span("notice warn", `origin(${plan.origin})이 GitHub 레포가 아닙니다. 이 창에서는 GitHub 레포만 연결합니다.`));
  }
  return nodes;
}

/* ── 지금 상태 ──────────────────────────────────────── */

/// 지금 이 레포가 어디에, 어떤 키로 붙어 있는가. 연결하기 전에 먼저 보여 준다.
function summary(plan) {
  const box = document.createElement("dl");
  box.className = "git-summary";
  const origin =
    plan.git === "absent" ? "git 저장소가 아닙니다" : plan.git === "local" ? "없음 — 원격 레포가 연결되지 않았습니다" : plan.repo ?? plan.origin;
  const inUse = plan.keys.find((k) => k.in_use);
  const key = inUse
    ? `레포 전용 키 · ${inUse.purpose}${inUse.usable ? "" : " (GitHub 등록 미완료)"}`
    : plan.current_key
      ? `시크릿 저장소 밖의 키 · ${plan.current_key}`
      : plan.git === "remote"
        ? "지정 없음 — 계정 기본 SSH 키로 접속합니다"
        : "—";
  const usable = plan.keys.filter((k) => k.usable).length;
  const stored = plan.git === "remote" ? `${plan.keys.length}개 (쓸 수 있는 키 ${usable}개)` : "레포를 정한 뒤 찾습니다";
  for (const [label, value] of [["origin", origin], ["지금 쓰는 키", key], ["저장된 키", stored]]) {
    const dt = document.createElement("dt");
    dt.textContent = label;
    const dd = document.createElement("dd");
    dd.textContent = value;
    box.append(dt, dd);
  }
  return box;
}

/* ── 창 ─────────────────────────────────────────────── */

function body(plan, project, close, reload) {
  const blockers = checks(plan, project, reload);
  const state = {
    account: plan.accounts[0]?.slug ?? "",
    remote: plan.git === "remote" ? "current" : "create",
    key: plan.keys.find((k) => k.in_use && k.usable)?.purpose ?? plan.keys.find((k) => k.usable)?.purpose ?? "issue",
  };

  // 레포 소유자는 계정을 바꾸면 따라 바뀐다.
  const login = plan.accounts[0]?.login ?? "";
  const owner = textInput(login, "소유자");

  // 계정
  const accountBox = document.createElement("div");
  accountBox.className = "choice-list";
  for (const account of plan.accounts) {
    const { row, input } = radio("git-account", account.slug, account.slug === state.account, account.login, `계정 ${account.slug}`);
    input.addEventListener("change", () => {
      state.account = account.slug;
      owner.value = account.login;
    });
    accountBox.append(row);
  }

  // 레포
  const owners = document.createElement("datalist");
  owners.id = "git-owners";
  for (const name of new Set([...plan.accounts.map((a) => a.login), ...plan.owners])) {
    const option = document.createElement("option");
    option.value = name;
    owners.append(option);
  }
  owner.setAttribute("list", owners.id);
  const repoName = textInput("", "레포 이름");
  const privacy = document.createElement("div");
  privacy.className = "choice-inline";
  const priv = radio("git-visibility", "private", true, "비공개");
  const pub = radio("git-visibility", "public", false, "공개");
  privacy.append(priv.row, pub.row);
  const existingUrl = textInput("", "git@github.com:owner/repo.git 또는 owner/repo");

  let repoSection;
  if (plan.git === "remote") {
    repoSection = section(
      "레포",
      span("mono", plan.repo ?? plan.origin),
      span("muted small", "이미 설정된 origin을 그대로 씁니다. 다른 레포로 바꾸는 일은 이 창에서 하지 않습니다."),
    );
  } else {
    const create = radio("git-remote", "create", true, "새 레포 만들기", "선택한 계정으로 GitHub에 만듭니다.");
    const existing = radio("git-remote", "existing", false, "기존 레포 연결", "GitHub에 이미 있는 레포를 origin으로 설정합니다.");
    const createFields = document.createElement("div");
    createFields.className = "choice-fields";
    const slugLine = document.createElement("div");
    slugLine.className = "slug-line";
    slugLine.append(owner, span("muted", "/"), repoName);
    createFields.append(slugLine, owners, privacy);
    const existingFields = document.createElement("div");
    existingFields.className = "choice-fields";
    existingFields.append(existingUrl);
    existingFields.hidden = true;
    create.input.addEventListener("change", () => {
      state.remote = "create";
      createFields.hidden = false;
      existingFields.hidden = true;
    });
    existing.input.addEventListener("change", () => {
      state.remote = "existing";
      createFields.hidden = true;
      existingFields.hidden = false;
    });
    const note = plan.git === "absent" ? span("muted small", "이 디렉토리는 아직 git 저장소가 아니라 먼저 git init을 합니다.") : null;
    repoSection = section("레포", ...(note ? [note] : []), create.row, createFields, existing.row, existingFields);
  }

  // 키 — 대상 레포가 바뀌면 그 레포의 저장된 키로 다시 그린다.
  const keyBox = document.createElement("div");
  keyBox.className = "choice-list";
  const purpose = textInput("develop", "용도");
  const keyStatus = span("key-status", "");
  function renderKeys(keys, target) {
    const usable = keys.filter((k) => k.usable);
    const keep = usable.find((k) => k.purpose === state.key);
    state.key = keep?.purpose ?? usable.find((k) => k.in_use)?.purpose ?? usable[0]?.purpose ?? "issue";

    if (!target) {
      keyStatus.textContent = "레포를 정하면 그 레포의 저장된 키를 찾습니다.";
    } else if (!keys.length) {
      keyStatus.textContent = `${target}의 저장된 키가 없습니다. 새로 발급합니다.`;
    } else {
      keyStatus.textContent = `${target}의 저장된 키 ${keys.length}개 — 쓸 수 있는 키 ${usable.length}개`;
    }

    const rows = [];
    for (const key of keys) {
      const note = [
        key.write ? "쓰기" : "읽기 전용",
        `계정 ${key.account}`,
        key.in_use ? "지금 이 레포가 쓰는 키" : "",
        key.usable ? "" : "GitHub 등록이 끝나지 않아 쓸 수 없음",
      ]
        .filter(Boolean)
        .join(" · ");
      const { row, input } = radio("git-key", key.purpose, key.purpose === state.key, `저장된 키 · ${key.purpose}`, note);
      input.disabled = !key.usable;
      input.addEventListener("change", () => {
        state.key = key.purpose;
      });
      rows.push(row);
    }
    const taken = keys.map((k) => k.purpose);
    const issue = radio(
      "git-key",
      "issue",
      state.key === "issue",
      "새 키 발급",
      "이 레포에만 쓰기 권한이 있는 배포 키를 만들어 GitHub에 등록합니다. 키는 시크릿 저장소에 둡니다." +
        (taken.length ? ` 이미 있는 용도(${taken.join(", ")})와 다른 이름을 쓰세요.` : ""),
    );
    issue.input.addEventListener("change", () => {
      state.key = "issue";
    });
    const purposeLine = document.createElement("div");
    purposeLine.className = "choice-fields";
    purposeLine.append(labeled("용도", purpose));
    keyBox.replaceChildren(...rows, issue.row, purposeLine);
  }

  /// 입력한 레포의 저장된 키를 찾는다. 입력이 멈춘 뒤 한 번만.
  let lookup;
  function lookupKeys(slug) {
    clearTimeout(lookup);
    if (!slug) {
      renderKeys([], null);
      return;
    }
    lookup = setTimeout(async () => {
      try {
        renderKeys(await invoke("repo_keys", { repo: slug }), slug);
      } catch {
        renderKeys([], null);
      }
    }, 300);
  }

  const current = plan.current_key && !plan.current_key_in_vault
    ? span("muted small", `지금은 시크릿 저장소 밖의 키(${plan.current_key})를 씁니다. 연결하면 선택한 키로 바뀝니다.`)
    : null;

  const keySection = section("SSH 키", keyStatus, ...(current ? [current] : []), keyBox);
  if (plan.git === "remote") {
    renderKeys(plan.keys, plan.repo);
  } else {
    renderKeys([], null);
    const targetOf = () =>
      state.remote === "create"
        ? owner.value.trim() && repoName.value.trim()
          ? `${owner.value.trim()}/${repoName.value.trim()}`
          : ""
        : existingUrl.value.trim();
    const refresh = () => lookupKeys(targetOf());
    for (const el of [owner, repoName, existingUrl]) el.addEventListener("input", refresh);
    repoSection.addEventListener("change", refresh);
  }
  const wiring = span(
    "pane-note",
    "키는 이 레포의 git 설정(core.sshCommand)에만 지정합니다. ~/.ssh/config는 바꾸지 않습니다. 연결한 뒤 git ls-remote로 접속을 확인합니다. push는 하지 않습니다.",
  );

  // 실행
  const actions = document.createElement("div");
  actions.className = "modal-actions";
  const problem = span("problem", "");
  problem.hidden = true;
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.textContent = "취소";
  cancel.addEventListener("click", close);
  const submit = document.createElement("button");
  submit.type = "button";
  submit.className = "primary";
  submit.textContent = "연결";
  submit.disabled = blockers.length > 0;
  submit.addEventListener("click", async () => {
    problem.hidden = true;
    submit.disabled = true;
    submit.textContent = "연결 중…";
    const form = {
      project: project.name,
      account: state.account,
      remote: {
        kind: state.remote,
        url: existingUrl.value,
        owner: owner.value,
        name: repoName.value,
        private: priv.input.checked,
      },
      key: state.key === "issue" ? { kind: "issue", purpose: purpose.value } : { kind: "stored", purpose: state.key },
    };
    try {
      const linked = await invoke("connect_git", { form });
      if (linked.unreachable) {
        problem.textContent = `설정은 끝났지만 접속을 확인하지 못했습니다: ${linked.unreachable} 작업 로그에서 자세한 내용을 확인하세요.`;
        problem.hidden = false;
        submit.textContent = "다시 확인";
        submit.disabled = false;
        return;
      }
      close();
    } catch (err) {
      problem.textContent = String(err);
      problem.hidden = false;
      submit.textContent = "연결";
      submit.disabled = false;
    }
  });
  actions.append(problem, cancel, submit);

  const parts = [summary(plan), ...blockers];
  if (plan.accounts.length) parts.push(section("GitHub 계정", accountBox));
  parts.push(repoSection, keySection, wiring, actions);
  return parts;
}

/// 프로젝트의 Git 연결 창을 연다.
export function openGit(project) {
  modal(`Git 연결 · ${project.name}`, (close) => {
    const holder = document.createElement("div");
    holder.className = "git-form";
    async function reload() {
      holder.replaceChildren(span("muted", "레포와 키를 확인하는 중…"));
      try {
        const plan = await invoke("git_plan", { name: project.name });
        holder.replaceChildren(...body(plan, project, close, reload));
      } catch (err) {
        holder.replaceChildren(span("problem", String(err)));
      }
    }
    reload();
    return [holder];
  });
}
