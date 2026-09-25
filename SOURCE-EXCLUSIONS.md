# Source exclusions

Excluded: generated outputs, backups, model/session/runtime databases, secrets, logs, binary build artifacts, private acceptance records and non-whitelisted config files.

Whitelisted configuration:
- config/package-launcher.json
- config/package-manifest.json

Special source allowlist:
- .coolzhu/plugins (Cargo workspace plugin crates only)
- docs files listed by docs/github-public-docs.json
