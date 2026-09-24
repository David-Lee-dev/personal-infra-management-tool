// 기타에 담는 것의 종류. 전부 다시 받을 수 없는 파일이다.

export const KINDS = [
  { id: "android", label: "Android 업로드 키" },
  { id: "apple", label: "Apple API 키" },
  { id: "service", label: "서비스 계정 JSON" },
  { id: "file", label: "그 밖의 파일" },
];

export function kindLabel(id) {
  return KINDS.find((k) => k.id === id)?.label ?? id;
}
