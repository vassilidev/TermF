// A stand-in for curl: TermF's list of releases comes from the scenario's file ($TERMF_E2E_RELEASES), anything
// else fails as an unreachable host would. No request ever leaves the machine.
import { appendFileSync, existsSync, readFileSync } from "node:fs";

const args = process.argv.slice(2);
if (process.env.TERMF_E2E_LOG) appendFileSync(process.env.TERMF_E2E_LOG, `curl ${JSON.stringify(args)}\n`);

const releases = process.env.TERMF_E2E_RELEASES;
if (args.includes("--version")) {
  process.stdout.write("curl 8.7.1 (e2e)\n");
} else if (args.some((a) => a.includes("api.github.com/repos/vassilidev/TermF/releases"))) {
  if (releases && existsSync(releases)) process.stdout.write(readFileSync(releases));
  else {
    process.stderr.write("curl: (22) The requested URL returned error: 404\n");
    process.exitCode = 22;
  }
} else {
  process.stderr.write("curl: (6) Could not resolve host (e2e: no network)\n");
  process.exitCode = 6;
}
