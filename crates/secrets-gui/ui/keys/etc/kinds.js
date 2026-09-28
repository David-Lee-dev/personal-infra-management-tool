// 기타에 담는 것의 종류. 전부 다시 받을 수 없는 파일이다.
// values 는 들일 때 미리 채워 두는 여는 값의 이름이다.

export const KINDS = [
  { id: "android", label: "Android 업로드 키", values: ["storePassword", "keyPassword", "keyAlias"] },
  { id: "apple", label: "Apple API 키", values: [] },
  { id: "apple-ads", label: "Apple Ads API 키", values: ["clientId", "teamId", "keyId"] },
  { id: "service", label: "서비스 계정 JSON", values: [] },
  { id: "file", label: "기타 파일", values: [] },
];

export function kindLabel(id) {
  return KINDS.find((k) => k.id === id)?.label ?? id;
}
