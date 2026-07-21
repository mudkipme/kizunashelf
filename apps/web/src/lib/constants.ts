export const allOptions = "all";
export const allTypes = "all";
export const defaultSort = "title";
// Server-side match-quality ranking (core `sort=relevance`). Used as the implicit
// default sort while a search query is active, and never persisted as a per-type
// preference — it's meaningless without a query.
export const relevanceSort = "relevance";
export const defaultDirection = "asc";
export const defaultView = "list";
export const pageSize = 40;
