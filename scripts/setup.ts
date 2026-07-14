/**
 * Cross-platform setup dispatcher (Deno).
 * Prefer: deno task setup
 * npm still has setup:linux / setup:mac / setup:windows.
 */
import { dirname, fromFileUrl, join } from "jsr:@std/path@1";

const rootPath = join(dirname(fromFileUrl(import.meta.url)), "..");

async function run(cmd: string, args: string[]) {
  const p = new Deno.Command(cmd, {
    args,
    cwd: rootPath,
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
  });
  const { code, success } = await p.output();
  if (!success) Deno.exit(code || 1);
}

const os = Deno.build.os;
if (os === "linux") {
  await run("bash", [join(rootPath, "scripts/fetch-sidecars-linux.sh")]);
} else if (os === "darwin") {
  await run("bash", [join(rootPath, "scripts/fetch-sidecars-macos.sh")]);
} else if (os === "windows") {
  const ps1 = join(rootPath, "scripts", "fetch-sidecars-windows.ps1");
  try {
    await run("pwsh", ["-File", ps1]);
  } catch {
    await run("powershell", ["-File", ps1]);
  }
} else {
  console.error(`Unsupported OS: ${os}`);
  Deno.exit(1);
}
