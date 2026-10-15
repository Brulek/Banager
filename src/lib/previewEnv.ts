/** Defense for mock/older payloads; Rust runner/redact.rs masks before IPC.
 * Mask only URL authority userinfo, leaving paths/queries containing @ alone.
 */
export function previewEnvValue(name: string, value: string): string {
  if (name !== "OLLAMA_HOST") return value;
  return value.replace(/^(?:[A-Za-z][A-Za-z0-9+.\-]*:\/\/)?[^/?#]*@/, (login) => {
    const scheme = login.match(/^[A-Za-z][A-Za-z0-9+.\-]*:\/\//)?.[0] ?? "";
    return `${scheme}${login.slice(scheme.length).includes(":") ? "****:****" : "****"}@`;
  });
}
