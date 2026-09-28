// 기타 항목의 상세.
//
// 파일 · 그 파일을 여는 값 · 쓰는 법 · 소비처. 지우는 버튼은 없다 — 다시 받을 수
// 없는 것이다. 소비처는 기록만 한다. 그 파일은 사람이 고친다.

import { button, facts, pane, path, span } from "../../dom.js";
import { chooser } from "../../combo.js";
import { ask, back, command, expiryField, head, purposeField, side } from "../parts.js";
import { row, table } from "../table.js";
import { kindLabel } from "./kinds.js";
import { etcId } from "./list.js";
import { projectChip, useOfPlace, usesOf } from "../usage.js";

const { invoke } = window.__TAURI__.core;

function whereOf(item) {
  return { project: item.project, name: item.name };
}

// 꺼낸 값은 화면에 두지 않고 클립보드로만 보낸다.
function copyButton(label, load) {
  const el = button(label, {
    onClick: async () => {
      try {
        await navigator.clipboard.writeText(await load());
        el.textContent = "복사됨";
        setTimeout(() => (el.textContent = label), 1500);
      } catch {
        /* 터미널 칸에 이미 남았다 */
      }
    },
  });
  return el;
}

const LOCAL = "local";

function hostLabel(host) {
  return host === LOCAL ? "로컬" : host;
}

function mono(text) {
  const el = span("mono", text);
  el.title = text;
  return el;
}

function size(bytes) {
  return bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KB`;
}

// 누르면 한 번 더 묻는다.
function armed(label, confirm, action) {
  const el = document.createElement("button");
  el.type = "button";
  el.textContent = label;
  let ready = false;
  const disarm = () => {
    ready = false;
    el.textContent = label;
    el.classList.remove("armed");
  };
  el.addEventListener("click", () => {
    if (!ready) {
      ready = true;
      el.textContent = confirm;
      el.classList.add("armed");
      setTimeout(disarm, 4000);
      return;
    }
    disarm();
    action();
  });
  return el;
}

function vaultFile(item) {
  return `${item.absolute}/files/${item.file.name}`;
}

function filePane(item) {
  return pane(
    "파일",
    facts([
      ["이름", item.file.name, true],
      ["크기", size(item.file.size)],
      ["SHA-256", `${item.file.sha256}…`, true],
      ["가져온 날", item.file.adopted_at],
      ["저장 위치", path(`${item.path}/files/${item.file.name}`), true],
    ]),
  );
}

function itemPane(item) {
  return pane(
    "항목",
    facts([
      ["프로젝트", item.project],
      ["이름", item.name, true],
      ["종류", kindLabel(item.kind)],
      ["용도", purposeField(item.purpose, (to) => ask("set_etc_purpose", { at: whereOf(item), to }))],
      ["만료", expiryField(item.expiry, (to) => ask("set_etc_expires", { at: whereOf(item), to }))],
    ]),
  );
}

// 값은 이름만 보이고 가린다. 꺼내는 건 복사로만 한다.
function valuesPane(item) {
  const box = pane(`포함된 값 · ${item.values.length}`);
  if (item.kind === "android") {
    box.querySelector(".pane-head").append(
      copyButton("key.properties 로 복사", () => ask("etc_key_properties", { at: whereOf(item) })),
    );
  }
  box.append(
    table(
      [
        { label: "이름", width: "34%" },
        { label: "값", width: "46%" },
        { label: "", width: "20%" },
      ],
      item.values.map((name) => {
        const cell = document.createElement("div");
        cell.className = "cell-actions";
        cell.append(copyButton("복사", () => ask("etc_value", { at: whereOf(item), name })));
        return row([name, { node: span("masked", "••••••••") }, { node: cell }]);
      }),
    ),
  );
  return box;
}

// 옮긴 뒤 빌드가 이어지려면 설정 파일이 금고 쪽을 가리켜야 한다. 고치는 건 사람이 한다.
function usagePane(item) {
  const line =
    item.kind === "android" ? `storeFile=${vaultFile(item)}` : vaultFile(item);
  return pane("사용 방법", command(line));
}

/// 프로젝트의 파일이면 프로젝트 안의 경로로 짧게, 전체 경로는 툴팁으로.
function fileCell(consumer, use) {
  const el = mono(use ? use.file : consumer.file);
  el.title = consumer.file;
  return el;
}

// 소비처는 기록만 한다. 파일은 건드리지 않는다.
function consumersPane(item) {
  const uses = usesOf(etcId(item));
  const box = pane(`쓰는 곳 · ${item.consumers.length}`);
  const slot = document.createElement("div");
  slot.className = "consumer-slot";

  const add = button("＋ 추가", {
    onClick: () => {
      add.disabled = true;
      slot.replaceChildren(
        addForm(item, () => {
          slot.replaceChildren();
          add.disabled = false;
        }),
      );
      slot.querySelector("input")?.focus();
    },
  });
  box.querySelector(".pane-head").append(add);

  if (item.consumers.length) {
    box.append(
      table(
        [
          { label: "프로젝트", width: "22%" },
          { label: "호스트", width: "14%" },
          { label: "파일", width: "46%" },
          { label: "", width: "18%" },
        ],
        item.consumers.map((consumer) => {
          const use = useOfPlace(uses, consumer);
          const cell = document.createElement("div");
          cell.className = "cell-actions";
          cell.append(
            armed("제거", "사용 위치 기록에서 제거", () =>
              ask("remove_etc_consumer", {
                at: whereOf(item),
                place: { host: consumer.host, file: consumer.file },
              }).catch(() => {}),
            ),
          );
          return row([
            { node: use ? projectChip(use) : span("muted small", "—") },
            hostLabel(consumer.host),
            { node: fileCell(consumer, use) },
            { node: cell },
          ]);
        }),
      ),
    );
  } else {
    box.append(span("pane-note", "없습니다."));
  }
  box.append(slot);
  return box;
}

let hosts = null;

function knownHosts() {
  const here = { value: "로컬", detail: "로컬" };
  hosts ??= invoke("ssh_hosts")
    .then((found) => [
      here,
      ...found.map((h) => ({ value: h.alias, detail: [h.address, h.user].filter(Boolean).join(" · ") })),
    ])
    .catch(() => [here]);
  return hosts;
}

function input(id, placeholder) {
  const el = document.createElement("input");
  el.id = id;
  el.type = "text";
  el.autocomplete = "off";
  el.spellcheck = false;
  el.placeholder = placeholder;
  return el;
}

function labelled(text, id, control) {
  const wrap = document.createElement("div");
  wrap.className = "field";
  const label = document.createElement("label");
  label.textContent = text;
  label.htmlFor = id;
  wrap.append(label, control);
  return wrap;
}

function addForm(item, onDone) {
  const form = document.createElement("div");
  form.className = "consumer-form";

  const host = input("e-host", "서버 이름 또는 로컬");
  const hostLine = document.createElement("div");
  hostLine.className = "with-chooser";
  hostLine.append(host, chooser(host, { title: "호스트 고르기", load: knownHosts }));
  const file = input("e-file", "자격 증명을 사용할 파일 경로");

  const grid = document.createElement("div");
  grid.className = "consumer-grid two";
  grid.append(labelled("호스트", "e-host", hostLine), labelled("파일", "e-file", file));

  const actions = document.createElement("div");
  actions.className = "row-actions";
  actions.append(
    button("취소", { onClick: onDone }),
    button("기록", {
      primary: true,
      onClick: async () => {
        const typed = host.value.trim() || "로컬";
        if (!file.value.trim()) return;
        try {
          await ask("add_etc_consumer", {
            at: whereOf(item),
            place: { host: typed === "로컬" ? LOCAL : typed, file: file.value.trim() },
          });
          onDone();
        } catch {
          /* 터미널 칸에 이미 남았다 */
        }
      },
    }),
  );
  form.append(grid, actions);
  return form;
}

export function renderItem(mount, item) {
  // 쓰는 곳이 먼저다. 값 · 파일 · 기록은 그다음.
  const body = document.createElement("div");
  body.className = "detail-body";
  body.append(consumersPane(item));
  if (item.values.length) body.append(valuesPane(item));
  body.append(usagePane(item), side(filePane(item), itemPane(item)));

  mount.replaceChildren(
    back("기타"),
    head(`${item.project} / ${item.name}`, kindLabel(item.kind), {}),
    body,
  );
}
