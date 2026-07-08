export const allOptions = "all";
export const allTypes = "all";
// Fallback tags frontmatter key, used only until the config loads. The real key
// is configurable per vault (`tags.field`) and comes from `ConfigResponse.tagsField`;
// mirrors the core `DEFAULT_TAGS_FIELD`.
export const defaultTagsField = "tags";
export const defaultSort = "title";
// Server-side match-quality ranking (core `sort=relevance`). Used as the implicit
// default sort while a search query is active, and never persisted as a per-type
// preference — it's meaningless without a query.
export const relevanceSort = "relevance";
export const defaultDirection = "asc";
export const defaultView = "list";
export const pageSize = 40;
