/** A date as `YYYY-MM-DD` in the user's *local* timezone (the log flow's source
 * of truth — the server never assumes UTC "today"). */
export function todayLocal(date = new Date()): string {
  return [
    date.getFullYear(),
    String(date.getMonth() + 1).padStart(2, "0"),
    String(date.getDate()).padStart(2, "0"),
  ].join("-");
}

/** Parse a stored `YYYY-MM-DD` value (ignoring any trailing time) into a Date at
 * *local* midnight — never `new Date("YYYY-MM-DD")`, which is parsed as UTC and
 * shifts a day in negative-offset zones. Returns null for anything that isn't a
 * full, valid calendar date, so partial values (a bare `2024`) or free text can
 * fall back to being shown verbatim. Storage always stays ISO; this is for
 * display formatting only. */
export function parseIsoDateLocal(value: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})/.exec(value);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(year, month - 1, day);
  // Reject overflow (e.g. 2024-02-31 rolling forward into March).
  if (date.getFullYear() !== year || date.getMonth() !== month - 1 || date.getDate() !== day) {
    return null;
  }
  return date;
}
