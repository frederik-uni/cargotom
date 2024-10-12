# cargotom
## Features
### Code actions
- "Make Workspace dependency" => This will generate `{ workspace = true }` for the dependency
- "Expand dependency specification" => This will convert from `"0.1.0"` to `{ version = "0.1.0" }`
- "Collapse dependency specification" => This will convert from `{ version = "0.1.0" }` to `"0.1.0`
- "Open Docs" => opens docs.rs/...
- "Open crates.io" => opens crates.io/...
- "Upgrade" => will upgrade the dependency version to the latest version
- "Upgrade All " => will upgrade every dependency version to the latest version
- "Update All" => will run `cargo run`

### Code completion
#### Dependencies
- crate names(online/offline)
- crate versions(online/offline)
  - latest version
  - workspace = true if in workspace
- crate features(online/offline)
- feature key when version after the key `crate = "0.1.0"` => `crate = {ve"0.1.0"` to `crate = { version = "0.1.0" }`

### Features
- feature names
- possible dependencies

### Diagnostics
- check if crate needs update
- check if crate version exists
- check if crate features exist

## Plans
- diagnostics
  - code action make optional if not & used & warn
  - check if version is set & dep in workspace
  - check when workspace modules have dep overlap
  - check when feature duplicate
  - check if optional dep are used
- inlay hints for version used(Cargo.lock)
- hover over features for available features
