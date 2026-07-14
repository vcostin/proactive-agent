/**
 * Cross-platform frontend runner for Tauri beforeDev/beforeBuild.
 * Prefer Deno; fall back to npm. No bash required (Windows-friendly).
 *
 * Usage: deno run -A scripts/run-frontend.ts <dev|build|preview>
 */
const task = Deno.args[0];
if (!task || !["dev", "build", "preview"].includes(task)) {
  console.error("usage: run-frontend.ts <dev|build|preview>");
  Deno.exit(1);
}

async function tryCmd(cmd: string, args: string[]): Promise<boolean> {
  try {
    const p = new Deno.Command(cmd, {
      args,
      stdin: "inherit",
      stdout: "inherit",
      stderr: "inherit",
    });
    const status = await p.output();
    if (!status.success) Deno.exit(status.code || 1);
    return true;
  } catch {
    return false;
  }
}

if (await tryCmd("deno", ["task", task])) {
  // ran
} else if (await tryCmd("npm", ["run", task])) {
  // ran
} else {
  console.error(`Need deno or npm to run frontend task '${task}'`);
  Deno.exit(1);
}
