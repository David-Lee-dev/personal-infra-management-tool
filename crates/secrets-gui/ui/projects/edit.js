// 등록한 뒤의 수정과 제거 — 프로젝트와 환경의 기록만 고친다.
//
// 로컬 디렉토리 · 서버 · GitHub 레포 · 키는 건드리지 않는다. 빼는 것은 보관소로 옮긴다.
// 빼기는 무엇이 옮겨지고 무엇이 그대로인지 보여 준 뒤, 한 번 더 눌러야 실행된다.

import { pickOrType, span } from "../dom.js";
import { modal } from "../modal.js";
import { checkoutLine } from "./server.js";

const { invoke } = window.__TAURI__.core;

function textInput(value, { mono = true, placeholder = "" } = {}) {
  const input = document.createElement("input");
  input.type = "text";
  input.value = value;
  input.placeholder = placeholder;
  input.spellcheck = false;
  input.autocomplete = "off";
  if (mono) input.className = "mono";
  return input;
}

function field(label, control, help) {
  const box = document.createElement("div");
  box.className = "field";
  const el = document.createElement("label");
  el.textContent = label;
  box.append(el, control);
  if (help) box.append(span("field-help", help));
  return box;
}

function button(label, className = "") {
  const b = document.createElement("button");
  b.type = "button";
  b.textContent = label;
  if (className) b.className = className;
  return b;
}

/// 빼기 구역 — 옮겨지는 것 · 그대로인 것을 보여 주고, 두 번 눌러야 실행한다.
function removal({ title, moves, keeps, label, run }) {
  const box = document.createElement("section");
  box.className = "danger-zone";
  const h = document.createElement("h3");
  h.textContent = title;
  const list = (head, items) => {
    const ul = document.createElement("ul");
    for (const item of items) {
      const li = document.createElement("li");
      li.textContent = item;
      ul.append(li);
    }
    return [span("danger-zone-head", head), ul];
  };
  const status = span("muted small", "");
  const go = button(label, "danger");
  go.addEventListener("click", async () => {
    if (!go.classList.contains("armed")) {
      go.classList.add("armed");
      go.textContent = label + " — 한 번 더 누르면 실행";
      status.textContent = "";
      return;
    }
    go.disabled = true;
    try {
      await run(status);
    } catch (err) {
      status.className = "warn-text small";
      status.textContent = String(err);
      go.disabled = false;
      go.classList.remove("armed");
      go.textContent = label;
    }
  });
  box.append(h, ...list("보관소로 옮기는 것", moves), ...list("그대로 두는 것", keeps), go, status);
  return box;
}

/* ── 프로젝트 ─────────────────────────────────────────── */

/// 프로젝트 편집 창. `onRenamed(새 이름)` · `onUnregistered()` 로 목록 쪽에 알린다.
export function openProjectEdit(project, { groups, onRenamed, onUnregistered }) {
  modal(`프로젝트 편집 · ${project.name}`, (close) => {
    const name = textInput(project.name);
    const group = pickOrType(groups, { newLabel: "＋ 새 그룹", placeholder: "그룹 이름", selected: project.group });
    const path = textInput(project.path, { placeholder: "~/workspace/…" });
    const pick = button("Finder에서 고르기");
    const pathRow = document.createElement("div");
    pathRow.className = "with-chooser";
    pathRow.append(path, pick);

    const problem = span("problem", "");
    problem.hidden = true;
    pick.addEventListener("click", async () => {
      problem.hidden = true;
      try {
        const chosen = await invoke("pick_project_folder", { title: "프로젝트 디렉토리" });
        if (chosen) path.value = chosen;
      } catch (err) {
        problem.textContent = String(err);
        problem.hidden = false;
      }
    });

    const cancel = button("취소");
    cancel.addEventListener("click", close);
    const save = button("저장", "primary");
    save.addEventListener("click", async () => {
      problem.hidden = true;
      save.disabled = true;
      try {
        const saved = await invoke("update_project", {
          form: { project: project.name, name: name.value, group: group.value(), path: path.value },
        });
        close();
        if (saved.name !== project.name) onRenamed(saved.name);
      } catch (err) {
        problem.textContent = String(err);
        problem.hidden = false;
        save.disabled = false;
      }
    });
    const actions = document.createElement("div");
    actions.className = "modal-actions";
    actions.append(problem, cancel, save);

    const servers = project.environments.map((e) => `서버 ${e.name}: ${e.login}@${e.address}:${e.path}의 코드와 ${e.server_env_file}`);
    const danger = removal({
      title: "등록 해제",
      moves: ["이 프로젝트의 기록(연결된 환경 · 환경 변수 대응 · 배포 스크립트) → ~/.secrets/archive/projects/<시각>-" + project.name + "/"],
      keeps: [`로컬 디렉토리 ${project.path}`, ...servers, "GitHub 레포와 키 · 자격 증명"],
      label: "등록 해제",
      run: async () => {
        await invoke("unregister_project", { project: project.name });
        close();
        onUnregistered();
      },
    });

    return [
      field("이름", name),
      field("그룹", group.node),
      field("경로", pathRow, "디렉토리를 옮겼다면 새 위치를 고릅니다. 디렉토리는 옮기지 않고 기록만 바꿉니다. 자격 증명의 사용 위치 기록은 옛 경로 그대로이니 자격 증명 화면에서 고칩니다."),
      actions,
      danger,
    ];
  });
}

/* ── 환경 ─────────────────────────────────────────────── */

/// 서버 계정 드롭다운 — 인스턴스마다 묶는다. 값은 `인스턴스|계정`.
function seatSelect(instances, env) {
  const select = document.createElement("select");
  let found = false;
  for (const item of instances) {
    const group = document.createElement("optgroup");
    group.label = `${item.name || item.instance} · ${item.address}`;
    for (const account of item.accounts) {
      const option = document.createElement("option");
      option.value = item.instance + "|" + account.login;
      option.textContent = account.login + (account.admin ? " (sudo)" : "");
      if (item.instance === env.instance && account.login === env.login) {
        option.selected = true;
        found = true;
      }
      group.append(option);
    }
    select.append(group);
  }
  // 기록된 계정이 저장소에서 사라졌어도 무엇이었는지는 보이게 한다.
  if (!found) {
    const gone = document.createElement("option");
    gone.value = env.instance + "|" + env.login;
    gone.textContent = `${env.login} @ ${env.instance_name || env.instance} (시크릿 저장소에 없음)`;
    gone.selected = true;
    select.prepend(gone);
  }
  return select;
}

/// 환경 편집 창.
export function openEnvironmentEdit(project, environment) {
  const env = project.environments.find((e) => e.name === environment);
  modal(`환경 편집 · ${project.name} · ${environment}`, (close) => {
    const holder = document.createElement("div");
    holder.className = "git-form";
    holder.append(span("muted", "서버 계정을 읽는 중…"));
    invoke("server_plan", { project: project.name })
      .then((plan) => holder.replaceChildren(...environmentBody(project, env, plan.instances, close)))
      .catch((err) => holder.replaceChildren(span("problem", String(err))));
    return [holder];
  });
}

function environmentBody(project, env, instances, close) {
  const name = textInput(env.name);
  const seat = seatSelect(instances, env);
  const path = textInput(env.path, { placeholder: "/srv/…" });
  const branch = textInput(env.branch, { placeholder: "main" });

  const result = document.createElement("div");
  result.className = "detected";
  const problem = span("problem", "");
  problem.hidden = true;
  const cancel = button("취소");
  cancel.addEventListener("click", close);
  const save = button("저장", "primary");
  save.addEventListener("click", async () => {
    problem.hidden = true;
    save.disabled = true;
    save.textContent = "확인하는 중…";
    const [instance, login] = seat.value.split("|");
    try {
      const done = await invoke("update_environment", {
        form: {
          project: project.name,
          environment: env.name,
          name: name.value,
          instance,
          login,
          path: path.value,
          branch: branch.value,
        },
      });
      const read = done.checkout ? " 서버를 다시 읽었습니다 — " + checkoutLine(done.checkout) : "";
      result.replaceChildren(span("", "저장했습니다." + read));
      save.textContent = "닫기";
      save.disabled = false;
      save.onclick = close;
      cancel.hidden = true;
    } catch (err) {
      problem.textContent = String(err);
      problem.hidden = false;
      save.textContent = "저장";
      save.disabled = false;
    }
  });
  const actions = document.createElement("div");
  actions.className = "modal-actions";
  actions.append(problem, cancel, save);

  const keeps = [`서버 ${env.login}@${env.address}:${env.path}의 코드와 ${env.server_env_file}`];
  if (env.env_file) keeps.push(`로컬 ${env.env_file}`);
  const danger = removal({
    title: "환경 빼기",
    moves: [
      "이 환경의 기록(서버 계정 · 배포 경로 · 브랜치 · 환경 변수 대응)",
      env.deploy_script ? "배포 스크립트" : "배포 스크립트 (없음)",
      "→ ~/.secrets/archive/projects/" + project.name + "/environments/<시각>-" + env.name + "/",
    ],
    keeps,
    label: "환경 빼기",
    run: async () => {
      await invoke("remove_environment", { project: project.name, environment: env.name });
      close();
    },
  });

  return [
    field("환경 이름", name, "이름을 바꾸면 배포 스크립트도 새 이름 아래로 옮깁니다."),
    field("서버 계정", seat),
    field("배포 경로", path, "비어 있는 경로이거나 이 레포를 받아 둔 경로여야 합니다."),
    field("브랜치", branch),
    span("pane-note", "서버 계정이나 배포 경로를 바꾸면 저장하기 전에 그 계정으로 서버를 다시 읽어 확인합니다. 서버에는 쓰지 않습니다."),
    result,
    actions,
    danger,
  ];
}
