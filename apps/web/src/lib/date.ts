/** A date as `YYYY-MM-DD` in the user's *local* timezone (the log flow's source
 * of truth — the server never assumes UTC "today"). */
export function todayLocal(date = new Date()): string {
  return [
    date.getFullYear(),
    String(date.getMonth() + 1).padStart(2, "0"),
    String(date.getDate()).padStart(2, "0"),
  ].join("-");
}
