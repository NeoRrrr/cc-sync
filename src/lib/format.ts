/* 压缩路径，只保留末尾两段，避免过长。 */
export function compactPath(value: string): string {
  const normalized = value.replace(/\\/g, "/");
  const parts = normalized.split("/").filter(Boolean);
  if (parts.length <= 2) {
    return value;
  }
  return `.../${parts.slice(-2).join("/")}`;
}

/* 压缩 skill 来源路径，保留末尾四段。 */
export function compactSkillPath(value: string): string {
  const normalized = value.replace(/\\/g, "/");
  const parts = normalized.split("/").filter(Boolean);
  if (parts.length <= 4) {
    return value;
  }
  return `.../${parts.slice(-4).join("/")}`;
}

/* skill 名已单独展示，这里只保留到 skills 根目录。 */
export function toSkillSourceRoot(value: string): string {
  const normalized = value.replace(/\\/g, "/");
  const parts = normalized.split("/").filter(Boolean);
  if (parts.length <= 1) {
    return value;
  }
  return parts.slice(0, -1).join("/");
}
