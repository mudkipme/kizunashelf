export const allOptions = "all";
export const allTypes = "all";
// The library browser is an unsaved smart list, so it sorts by Bases property
// reference (`file.name`, `file.mtime`, `note.<field>`) — see
// `components/smart-lists/sort-picker`.
export const defaultSort = "file.name";
// Server-side match-quality ranking (core `sort=relevance`) on the entity list
// endpoint, which the relation autocomplete uses. Browse gets the same ranking
// implicitly: a search with no explicit sort ranks by relevance.
export const relevanceSort = "relevance";
export const defaultDirection = "asc";
export const defaultView = "list";
export const pageSize = 40;
