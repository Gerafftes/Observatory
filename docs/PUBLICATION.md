# Public source boundary

Observatory publishes editable software, hardware, documentation, public media,
and tests. A full local project also contains private evidence and backups;
those are deliberately outside the publication tree.

## Keep private

- Credentials, `.env` files, provisioning exports, NVS/flash dumps, WiFi passwords,
  control tokens, private keys, raw recordings, packet captures, and databases.
- `private/`, `artifacts/`, `devlogs/`, `.toolchains/`, dependency/build caches,
  local agent settings, session memories, backups, and migration reports.
- Desktop screenshots containing personal hostnames, browsing activity, or
  private command lines. Keep the original in private evidence storage.
- Actual device identities and machine-specific paths. Use explicit local
  configuration for operation and synthetic identities for examples/tests.

Safe example configuration and required runtime source, including vendored
source, may be public. Keep all applicable license and notice files.
`.gitignore` only affects untracked files: it does not remove files already
tracked in Git. Review the actual proposed index as well as the worktree.

## Before publication

1. Inspect the complete proposed diff and the list of files to be published;
   preserve unrelated local edits and private originals.
2. Run `sh project_tools/verify_observatory_source.sh` and
   `python3 project_tools/check_publication.py --secrets` in the prepared checkout.
   Gitleaks must be installed. Secret findings are redacted; unexpected findings
   block the check. The fixture baseline is specific to rule, line, and complete
   file hash, never a blanket exception for tests or vendored code.
3. Inspect images and archive contents for private data. EXIF checks cannot see
   credentials or identities drawn into pixels. Do not publish audit reports
   containing detected values.
4. Scan the Git history/commit range separately. Removing a value in a new commit
   leaves it in old commits. If a credential may be real, its owner must revoke
   or rotate it; do not test its validity against the service without permission.
5. Recheck local links, source inventory, simulation, browser tests, and affected
   hardware-free tests. Record missing dependencies and unverified live behavior.

`SHA256SUMS.txt` describes the prepared source tree. It is a reproducibility
inventory, not a security guarantee or evidence that hardware experiments passed.
Rebuild it after intentional publication edits; never use it to overwrite private
historical evidence. A later upload requires a separate explicit instruction.
