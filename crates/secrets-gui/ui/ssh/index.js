// SSH 접속 — 별칭마다 어느 인스턴스의 어느 계정인지 정한다. 그룹마다 conf 파일이 만들어지고,
// ~/.ssh/config 는 Include 한 줄로 그 파일들을 합친다. 별칭 · 그룹 · 계정은 사용자가 정한다.

import { pickOrType, span } from "../dom.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const body = document.getElementById("ssh-body");

/// [Include 넣기] 뒤에 보관한 원본 자리. 화면을 다시 그려도 알려 주려고 들고 있는다.
let keptOriginal = null;

function includePane(view) {
  const box = document.createElement("section");
  box.className = "project-pane";
  const title = document.createElement("h2");
  title.textContent = "~/.ssh/config";
  box.append(title);

  const line = span("mono small", view.include_line);
  if (view.includes_ours) {
    const count = view.groups.reduce((n, g) => n + g.hosts.length, 0);
    const state = count
      ? `${view.user_config} 맨 위에 Include가 있어 아래 별칭 ${count}개가 쓰입니다.`
      : `${view.user_config} 맨 위에 Include가 있습니다. 아직 별칭이 없어 합칠 설정이 없습니다 — 아래에서 별칭을 추가하면 바로 쓰입니다.`;
    box.append(span("", state), line);
    if (keptOriginal) box.append(span("muted small", `넣기 전 원본: ${keptOriginal}`));
  } else {
    const note = span(
      "notice warn",
      `${view.user_config}에 아래 한 줄이 없어 이 설정이 쓰이지 않습니다. 버튼을 누르면 원본을 보관소에 복사한 뒤 맨 위에 이 한 줄만 넣습니다.`,
    );
    const add = document.createElement("button");
    add.type = "button";
    add.className = "primary";
    add.textContent = "Include 넣기";
    const result = span("muted small", "");
    add.addEventListener("click", async () => {
      add.disabled = true;
      try {
        keptOriginal = await invoke("add_ssh_include");
        load();
      } catch (err) {
        result.textContent = String(err);
        add.disabled = false;
      }
    });
    box.append(note, line, add, result);
  }
  if (view.duplicates.length) {
    box.append(
      span(
        "notice warn",
        `직접 쓴 ${view.user_config}에도 있는 별칭: ${view.duplicates.join(", ")}. Include가 맨 위에 있으면 이 설정이 먼저 쓰입니다. 직접 쓴 블록은 지워도 됩니다.`,
      ),
    );
  }
  return box;
}

function groupPane(group) {
  const box = document.createElement("section");
  box.className = "project-pane";
  const head = document.createElement("div");
  head.className = "ssh-group-head";
  const title = document.createElement("h2");
  title.textContent = group.group;
  head.append(title, span("mono small muted", group.file));
  box.append(head);

  const table = document.createElement("table");
  table.className = "env-table";
  const thead = document.createElement("thead");
  const tr = document.createElement("tr");
  for (const label of ["별칭", "인스턴스", "계정", "주소", ""]) {
    const th = document.createElement("th");
    th.textContent = label;
    tr.append(th);
  }
  thead.append(tr);
  const tbody = document.createElement("tbody");
  for (const host of group.hosts) {
    const row = document.createElement("tr");
    const remove = document.createElement("button");
    remove.type = "button";
    remove.textContent = "빼기";
    remove.title = "별칭만 뺍니다. 서버 계정과 키는 그대로입니다.";
    remove.addEventListener("click", async () => {
      remove.disabled = true;
      try {
        render(await invoke("remove_ssh_host", { alias: host.alias }));
      } catch (err) {
        remove.disabled = false;
        remove.title = String(err);
      }
    });
    const cells = [
      span("mono strong", host.alias),
      span("", host.instance_name || host.instance),
      span("mono", host.login),
      host.found ? span("mono small", host.address) : span("warn-text small", "시크릿 저장소에 이 계정이 없습니다"),
      remove,
    ];
    for (const node of cells) {
      const td = document.createElement("td");
      td.append(node);
      row.append(td);
    }
    tbody.append(row);
  }
  table.append(thead, tbody);
  box.append(table);
  return box;
}

function field(label, control) {
  const box = document.createElement("div");
  box.className = "field";
  const el = document.createElement("label");
  el.textContent = label;
  box.append(el, control);
  return box;
}

function addPane(view) {
  const box = document.createElement("section");
  box.className = "project-pane";
  const title = document.createElement("h2");
  title.textContent = "별칭 추가";
  box.append(title);
  if (!view.instances.length) {
    box.append(span("muted", "시크릿 저장소에 서버 계정이 없습니다. 자격 증명 › AWS에서 계정을 만드세요."));
    return box;
  }

  // 별칭은 인스턴스 이름과 계정 이름으로 자동으로 만들어진다. 고치면 그 값을 쓴다.
  const alias = document.createElement("input");
  alias.type = "text";
  alias.className = "mono";
  alias.spellcheck = false;
  let aliasEdited = false;
  alias.addEventListener("input", () => {
    aliasEdited = true;
  });
  const group = pickOrType(view.known_groups, { newLabel: "＋ 새 그룹", placeholder: "그룹 — conf 파일 이름" });

  const instance = document.createElement("select");
  const login = document.createElement("select");
  const taken = new Set(view.groups.flatMap((g) => g.hosts.map((h) => h.alias)));
  function fillAlias() {
    if (aliasEdited) return;
    const found = view.instances.find((i) => i.instance === instance.value);
    alias.value = found?.accounts.find((a) => a.login === login.value)?.alias ?? "";
  }
  function fillLogins() {
    const found = view.instances.find((i) => i.instance === instance.value);
    login.replaceChildren(
      ...(found?.accounts ?? []).map((account) => {
        const option = document.createElement("option");
        option.value = account.login;
        option.textContent = taken.has(account.alias) ? `${account.login} (별칭 있음)` : account.login;
        return option;
      }),
    );
    fillAlias();
  }
  login.addEventListener("change", fillAlias);
  for (const i of view.instances) {
    const option = document.createElement("option");
    option.value = i.instance;
    option.textContent = `${i.name || i.instance} · ${i.address}`;
    instance.append(option);
  }
  instance.addEventListener("change", fillLogins);
  fillLogins();

  const problem = span("problem", "");
  problem.hidden = true;
  const add = document.createElement("button");
  add.type = "button";
  add.className = "primary";
  add.textContent = "추가";
  add.addEventListener("click", async () => {
    problem.hidden = true;
    add.disabled = true;
    try {
      render(
        await invoke("add_ssh_host", {
          form: { alias: alias.value, group: group.value(), instance: instance.value, login: login.value },
        }),
      );
    } catch (err) {
      problem.textContent = String(err);
      problem.hidden = false;
      add.disabled = false;
    }
  });

  const grid = document.createElement("div");
  grid.className = "ssh-add-grid";
  grid.append(field("인스턴스", instance), field("계정", login), field("그룹", group.node), field("별칭", alias));
  const actions = document.createElement("div");
  actions.className = "modal-actions";
  actions.append(problem, add);
  box.append(grid, actions);
  return box;
}

function render(view) {
  const head = document.createElement("div");
  head.className = "project-bar";
  const title = document.createElement("h1");
  title.textContent = "SSH 접속";
  head.append(title, span("project-count", "별칭 · 그룹은 직접 정합니다. 주소와 키는 시크릿 저장소의 서버 계정에서 가져옵니다."));

  const grid = document.createElement("div");
  grid.className = "ssh-grid";
  grid.append(includePane(view));
  for (const group of view.groups) grid.append(groupPane(group));
  if (!view.groups.length) grid.append(span("list-none", "아직 별칭이 없습니다."));
  grid.append(addPane(view));
  body.replaceChildren(head, grid);
}

export async function loadSsh() {
  try {
    render(await invoke("ssh_overview"));
  } catch (err) {
    body.replaceChildren(span("problem", String(err)));
  }
}

const load = loadSsh;

listen("ssh:updated", () => {
  if (!body.closest("[hidden]")) loadSsh();
});
