# Security Policy

Observatory is an experimental, camera-free WiFi-CSI and mmWave reference
project. The repository contains documentation, embedded firmware, sensing
services, and web interfaces. Sensor data can reveal presence, movement,
position, or vital-sign-related signals, so security reports and attached
captures must be handled carefully.

## Supported versions

Only the latest state of the default `main` branch is actively considered for
security fixes. Feature branches, historical snapshots, and downstream forks
may not contain the current hardening. Maintainers of forks are responsible for
backporting fixes to their own releases.

## Reporting a vulnerability

Please do not open a public GitHub issue or include exploit details in a public
pull request.

Use GitHub's private vulnerability reporting flow from the repository's
**Security** tab when it is available. If private reporting is unavailable,
contact the repository maintainers through a private GitHub channel before
disclosing the details publicly.

Please include, where possible:

- the affected component, path, commit, or release;
- the security impact and the access or deployment conditions required;
- clear reproduction steps or a minimal proof of concept; and
- logs, traces, or suggested mitigations that help reproduce and triage the
  issue.

Do not include WiFi passwords, bearer tokens, private keys, raw CSI or mmWave
captures, personal data, or other secrets in a report. Start with sanitized
metadata and provide additional evidence only through a private channel.

We will triage credible reports, work with the reporter on a fix or mitigation,
and coordinate any public disclosure when appropriate. No fixed response or
disclosure timeline is promised.

## Scope and safe research

The in-scope attack surface includes code, firmware, sensing servers, web
interfaces, build and deployment scripts, dependency integration, and security
or privacy boundaries implemented in this repository.

When researching a suspected issue:

- test only devices, networks, and deployments that you own or are explicitly
  authorized to assess;
- avoid denial-of-service testing, destructive flashing, RF interference,
  credential access, or collection of another person's sensor data; and
- keep experimental sensing services on a protected local network and do not
  expose them to the public Internet while testing.

Issues that belong solely to an upstream dependency should also be reported to
that project's maintainers, with the local impact on Observatory included here
when relevant.
