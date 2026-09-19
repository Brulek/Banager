/** Display-only: quotes a token that contains whitespace, a quote or is empty,
 *  so the preview cannot blur where one argument ends and the next begins.
 *  Nothing here is ever executed — submission only ever sends a PlanId. */
export function displayToken(token: string): string {
  if (token === "") return "''";
  if (!/[\s"'\\]/.test(token)) return token;
  return `'${token.replace(/'/g, `'\\''`)}'`;
}
