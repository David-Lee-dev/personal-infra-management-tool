// 서버 등록 · 편집. 등록은 이 도구의 기록만 만든다 — 서버에는 아무것도 쓰지 않는다.

import { pickOrType, span } from "../dom.js";
import { modal } from "../modal.js";
import { setDomain } from "../keys/state.js";
import { showTab } from "../tabs.js";
import { termWrite } from "../terminal.js";
import { field, heldPems, keyChooser, segmented, textInput } from "./form.js";

const { invoke } = window.__TAURI__.core;

const KINDS = [["ec2", "EC2"], ["lightsail", "Lightsail"], ["other", "기타"]];
const NO_GROUP = "그룹 없음";

/// 서버의 속성 칸들 — 종류 · 이름 · 그룹 · 주소 · 포트 · AWS 사실 · 공용 자리 · 메모.
/// AWS 사실은 AWS 종류일 때만 보인다. pem 을 고르면 그 계정 · 리전이 채워진다.
function serverFields(initial, { groups, pems, onKind }) {
  const aws = initial.aws ?? { account: "", region: "", instance: "" };
  const name = textInput(initial.name ?? "");
  const address = textInput(initial.address ?? "", { placeholder: "IP 또는 DNS" });
  const port = textInput(String(initial.port ?? 22));
  port.inputMode = "numeric";
  const group = pickOrType([NO_GROUP, ...groups], { selected: initial.group || NO_GROUP, placeholder: "그룹 이름" });
  const account = textInput(aws.account, { placeholder: "12자리 AWS 계정 ID" });
  const region = textInput(aws.region, { placeholder: "AWS 리전 코드" });
  const instance = textInput(aws.instance, { placeholder: "인스턴스 ID (선택)" });
  const workspace = textInput(initial.workspace ?? "/srv");
  const workspaceGroup = textInput(initial.workspace_group ?? "workspace");
  const note = textInput(initial.note ?? "", { mono: false });

  const awsBox = document.createElement("div");
  awsBox.className = "sv-grid";
  awsBox.append(
    field("AWS 계정 ID", account),
    field("리전", region),
    field("인스턴스 ID (선택)", instance),
  );

  const kind = segmented(KINDS, initial.kind ?? "ec2", (k) => {
    awsBox.hidden = k === "other";
    onKind?.(k);
  });
  awsBox.hidden = kind.value() === "other";

  const main = document.createElement("div");
  main.className = "sv-grid";
  main.append(
    field("이름", name),
    field("그룹 (선택)", group.node),
    field("주소", address),
    field("포트", port),
  );
  const extra = document.createElement("div");
  extra.className = "sv-grid";
  extra.append(
    field("공용 작업 디렉터리", workspace, "이 서버에 계정을 만들 때 함께 쓰는 자리입니다."),
    field("공용 그룹", workspaceGroup),
    field("메모 (선택)", note),
  );

  return {
    kind,
    address,
    nodes: [field("종류", kind.node, "기타는 로컬 기기 · Tailscale · 다른 클라우드처럼 AWS 밖의 서버입니다."), main, awsBox, extra],
    /// pem 을 고르면 그 AWS 계정 · 리전을 채운다.
    fillFromPem(pem) {
      if (!pem) return;
      if (!account.value) account.value = pem.account;
      if (!region.value) region.value = pem.region;
    },
    value() {
      const k = kind.value();
      const picked = group.value();
      return {
        name: name.value.trim(),
        group: picked === NO_GROUP ? "" : picked.trim(),
        address: address.value.trim(),
        port: Number.parseInt(port.value, 10) || 0,
        kind: k,
        aws: k === "other" ? null : { account: account.value.trim(), region: region.value.trim(), instance: instance.value.trim() },
        workspace: workspace.value.trim(),
        workspace_group: workspaceGroup.value.trim(),
        note: note.value.trim(),
      };
    },
  };
}

function actions(close, label, run) {
  const box = document.createElement("div");
  box.className = "modal-actions";
  const problem = span("problem", "");
  problem.hidden = true;
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.textContent = "취소";
  cancel.addEventListener("click", close);
  const submit = document.createElement("button");
  submit.type = "button";
  submit.className = "primary";
  submit.textContent = label;
  submit.addEventListener("click", async () => {
    problem.hidden = true;
    submit.disabled = true;
    try {
      await run();
    } catch (err) {
      problem.textContent = String(err);
      problem.hidden = false;
      submit.disabled = false;
    }
  });
  box.append(problem, cancel, submit);
  return box;
}

/// 서버 등록 창. 처음 쓸 계정 하나와 함께 기록한다.
export function openRegister({ groups, onDone }) {
  modal("서버 등록", (close) => {
    const holder = document.createElement("div");
    holder.className = "git-form";
    holder.append(span("muted", "pem 키를 읽는 중…"));
    heldPems().then((pems) => holder.replaceChildren(...registerBody(pems, groups, close, onDone)));
    return [holder];
  }, { size: "lg" });
}

function registerBody(pems, groups, close, onDone) {
  const login = textInput("ubuntu");
  let role = "admin";
  const roleBox = segmented([["user", "사용자"], ["admin", "관리자 (sudo)"]], role, (r) => {
    role = r;
    adminBox.hidden = r !== "admin";
  });
  const purpose = textInput("", { mono: false, placeholder: "선택" });

  const keyHolder = document.createElement("div");
  let chooser;
  const server = serverFields({}, {
    groups,
    pems,
    onKind: (k) => drawKeys(k),
  });
  function drawKeys(kind) {
    const usable = kind === "other" ? [] : pems.filter((p) => p.machine === kind);
    const toPems = () => {
      close();
      showTab("keys");
      setDomain("pem");
    };
    chooser = keyChooser({ pems: usable.map((p) => p.name), onPemLink: kind === "other" ? null : toPems });
    keyHolder.replaceChildren(chooser.node);
    login.value = kind === "other" ? "" : login.value || "ubuntu";
  }
  drawKeys(server.kind.value());

  const admin = document.createElement("input");
  admin.type = "checkbox";
  admin.checked = true;
  const adminBox = document.createElement("label");
  adminBox.className = "check-line";
  adminBox.append(admin, span("", "이 계정을 관리 접속으로 지정 — 이 서버에 새 계정을 만들 때 이 계정으로 들어갑니다."));

  const account = document.createElement("section");
  account.className = "git-section";
  const h = document.createElement("h3");
  h.textContent = "처음 쓸 계정";
  const grid = document.createElement("div");
  grid.className = "sv-grid";
  grid.append(field("로그인", login), field("역할", roleBox.node), field("용도", purpose));
  account.append(h, span("pane-note", "접속에 쓸 계정 하나. 나머지 계정은 등록한 뒤 서버 상세에서 더합니다."), grid, keyHolder, adminBox);

  const section = document.createElement("section");
  section.className = "git-section";
  const sh = document.createElement("h3");
  sh.textContent = "서버";
  section.append(sh, ...server.nodes);

  return [
    span("pane-note", "등록은 이 도구의 기록만 만듭니다. 서버에는 아무것도 쓰지 않고, ~/.ssh/config도 바꾸지 않습니다."),
    section,
    account,
    actions(close, "등록", async () => {
      const key = chooser.value();
      if (key.kind === "pem") server.fillFromPem(pems.find((p) => p.name === key.value && p.machine === server.kind.value()));
      const done = await invoke("register_server", {
        form: {
          server: server.value(),
          account: { login: login.value.trim(), role, purpose: purpose.value.trim(), key },
          admin_access: role === "admin" && admin.checked,
        },
      });
      close();
      onDone?.(done.id);
      // 등록한 계정으로 한 번 들어가 본다. 실패해도 등록은 그대로이고, 이유는 작업 로그에 남는다.
      invoke("check_server_account", { id: done.id, login: done.accounts[0].login }).catch((err) =>
        termWrite("err", `${done.name}/${done.accounts[0].login} 접속 확인: ${err}`),
      );
    }),
  ];
}

/// 서버 편집 창. 계정은 그대로이고 속성만 고친다.
export function openEdit(server, { groups, onDone }) {
  modal(`서버 편집 · ${server.name}`, (close) => {
    const fields = serverFields(server, { groups, pems: [] });
    const admins = server.accounts.filter((a) => a.role === "admin").map((a) => a.login);
    const admin = document.createElement("select");
    for (const value of ["", ...admins]) {
      const option = document.createElement("option");
      option.value = value;
      option.textContent = value || "지정 안 함";
      admin.append(option);
    }
    admin.value = server.admin ?? "";

    const warning = span("notice warn", "");
    warning.hidden = true;
    fields.address.addEventListener("input", () => {
      const changed = fields.address.value.trim() !== server.address;
      warning.hidden = !changed;
      warning.textContent = `주소를 바꾸면 이 서버의 계정 ${server.accounts.length}개와 환경 ${server.uses.length}개가 새 주소로 접속합니다.`;
    });

    return [
      ...fields.nodes,
      field("관리 접속", admin, "이 서버에 새 계정을 만들 때 들어가는 관리자 계정입니다."),
      warning,
      actions(close, "저장", async () => {
        await invoke("update_server", { id: server.id, form: { ...fields.value(), admin: admin.value || null } });
        close();
        onDone?.();
      }),
    ];
  }, { size: "lg" });
}
