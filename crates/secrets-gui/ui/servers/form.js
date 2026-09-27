// 서버 화면의 입력 조각. 등록 · 편집 · 계정 추가 창이 같이 쓴다.

import { span } from "../dom.js";

const { invoke } = window.__TAURI__.core;

export const KIND_LABEL = { ec2: "EC2", lightsail: "Lightsail", other: "기타" };

export function textInput(value = "", { mono = true, placeholder = "" } = {}) {
  const input = document.createElement("input");
  input.type = "text";
  input.value = value;
  input.placeholder = placeholder;
  input.spellcheck = false;
  input.autocomplete = "off";
  if (mono) input.className = "mono";
  return input;
}

export function field(label, control, help) {
  const box = document.createElement("div");
  box.className = "field";
  const el = document.createElement("label");
  el.textContent = label;
  box.append(el, control);
  if (help) box.append(span("field-help", help));
  return box;
}

/// 분절 선택. `onPick(value)` 은 바뀔 때 부른다.
export function segmented(options, selected, onPick) {
  const box = document.createElement("div");
  box.className = "segmented";
  box.setAttribute("role", "tablist");
  let value = selected;
  const buttons = options.map(([id, label]) => {
    const b = document.createElement("button");
    b.type = "button";
    b.setAttribute("role", "tab");
    b.textContent = label;
    b.addEventListener("click", () => {
      value = id;
      for (const other of buttons) other.setAttribute("aria-selected", String(other === b));
      onPick?.(id);
    });
    b.setAttribute("aria-selected", String(id === selected));
    return b;
  });
  box.append(...buttons);
  return { node: box, value: () => value };
}

/// 시크릿 저장소에 가져온 pem. AWS 서버의 키 페어를 고르는 데 쓴다.
export async function heldPems() {
  try {
    return (await invoke("list_aws_keys")).keys;
  } catch {
    return [];
  }
}

/// 계정의 키 고르기 — pem(AWS 서버만) · 키 파일 경로 · 시크릿 저장소로 가져오기 · ssh 기본 키.
///
/// `pems` 는 이 서버에 쓸 수 있는 pem 이름들. 비어 있으면 pem 을 고를 수 없고, `onPemLink` 가 있으면
/// pem 을 가져오러 가는 링크를 단다.
export function keyChooser({ pems = [], initial = pems.length ? "pem" : "file", onPemLink = null } = {}) {
  const box = document.createElement("div");
  box.className = "sv-keys";
  const name = `key-${Math.random().toString(36).slice(2)}`;

  const pemSelect = document.createElement("select");
  for (const pem of pems) {
    const option = document.createElement("option");
    option.value = pem;
    option.textContent = pem;
    pemSelect.append(option);
  }
  const path = textInput("", { placeholder: "~/.ssh/…" });
  const pick = document.createElement("button");
  pick.type = "button";
  pick.textContent = "찾아보기";
  pick.addEventListener("click", async () => {
    const picked = await invoke("pick_key_file").catch(() => null);
    if (picked) path.value = picked;
  });
  const pathRow = span("with-chooser", "");
  pathRow.append(path, pick);

  const choices = [
    ["pem", "pem 키", "자격 증명 › 서버 키에 가져온 이 AWS 계정 · 리전의 pem.", pemSelect],
    ["file", "키 파일 가리키기", "로컬의 키 파일 경로만 기록합니다. 파일은 옮기지 않습니다.", null],
    ["import", "시크릿 저장소로 가져오기", "키 파일을 ~/.secrets 로 복사합니다(600). 원본은 그대로 둡니다.", null],
    ["agent", "ssh 기본 키", "키를 지정하지 않습니다 — ssh-agent · ~/.ssh/id_* 를 씁니다.", null],
  ];
  let kind = initial;
  const rows = [];
  for (const [id, label, note, extra] of choices) {
    const row = document.createElement("label");
    row.className = "choice-row";
    const radio = document.createElement("input");
    radio.type = "radio";
    radio.name = name;
    radio.value = id;
    radio.checked = id === kind;
    radio.disabled = id === "pem" && !pems.length;
    radio.addEventListener("change", () => {
      kind = id;
      sync();
    });
    const text = span("choice-text", "");
    text.append(span("strong", label), span("muted small", id === "pem" && !pems.length ? "이 서버에 쓸 수 있는 pem 이 없습니다." : note));
    row.append(radio, text);
    if (extra) row.append(extra);
    rows.push(row);
    box.append(row);
  }
  box.append(pathRow);
  if (!pems.length && onPemLink) {
    const go = document.createElement("button");
    go.type = "button";
    go.className = "link-button";
    go.textContent = "자격 증명 › 서버 키에서 pem 가져오기";
    go.addEventListener("click", onPemLink);
    box.append(go);
  }
  function sync() {
    pemSelect.disabled = kind !== "pem";
    pathRow.hidden = !(kind === "file" || kind === "import");
  }
  sync();

  return {
    node: box,
    value: () => ({ kind, value: kind === "pem" ? pemSelect.value : kind === "agent" ? "" : path.value.trim() }),
  };
}
