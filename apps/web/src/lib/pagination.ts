export function visiblePages(page: number, totalPages: number): Array<number | "ellipsis"> {
  if (totalPages <= 7) return Array.from({ length: totalPages }, (_, index) => index + 1);

  const pages = new Set([1, totalPages, page - 1, page, page + 1]);
  const ordered = [...pages]
    .filter((item) => item >= 1 && item <= totalPages)
    .sort((a, b) => a - b);
  const result: Array<number | "ellipsis"> = [];

  for (const item of ordered) {
    const previous = result[result.length - 1];
    if (typeof previous === "number" && item - previous > 1) result.push("ellipsis");
    result.push(item);
  }

  return result;
}
