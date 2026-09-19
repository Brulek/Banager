/** Display-only: renders one argv token so the preview cannot blur where one
 *  argument ends and the next begins. Uses an allow-list (the `shlex.quote`
 *  rule) rather than a deny-list, so a token carrying a shell metacharacter
 *  like `$`, `;` or a backtick is quoted too: an operator who copies this
 *  preview into a terminal must get the command they were shown.
 *  Nothing here is ever executed — submission only ever sends a PlanId. */
export function displayToken(token: string): string {
  if (token === "") return "''";
  if (/^[A-Za-z0-9_@%+=:,./-]+$/.test(token)) return token;
  return `'${token.replace(/'/g, `'\\''`)}'`;
}
