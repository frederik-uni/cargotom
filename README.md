# CargoTom[l]
## Config
```json
"lsp": {
  "cargo-tom": {
    "initialization_options": {
      /// Search
      "per_page": 25,
      /// What features should be displayed on hover
      /// All, UnusedOpt, Features,
      "feature_display_mode": "UnusedOpt",
      "hide_docs_info_message": false,
      /// Sort toml on format
      "sort_format": false,
      /// Use stable versions in completions
      "stable_version": true,
      /// Offline mode uses https://github.com/frederik-uni/crates.io-dump-minfied for search
      /// The search/order is non existent(name.starts_with) feel free to contribute
      "offline": false,
    }
  },
  ...
}
```
## Features
### Code actions
- [ ] Add self as dependency
- [x] Open LSP docs(first line)
- [x] Open LSP issues(first line)
- [x] Open Cargo.toml docs(first line)
- [x] "Make Workspace dependency" => This will generate `{ workspace = true }` for the dependency
- [x] "Expand dependency specification" => This will convert from `"0.1.0"` to `{ version = "0.1.0" }`
- [x] "Collapse dependency specification" => This will convert from `{ version = "0.1.0" }` to `"0.1.0`
- [x] "Open Docs" => opens docs.rs/...
- [x] "Open Homepage" => opens ???...
- [x] "Open crates.io" => opens crates.io/...
- [x] "Open Src code" => opens src code on github
- [x] "Upgrade" => will upgrade the dependency version to the latest version
- [ ] "Upgrade All" => will upgrade every dependency version to the latest version
- [x] "Update All" => will run `cargo update`
- [x] toggle optional dependency
- [ ] make dependency optional if in feature
- [x] fix missing in workspace


### Inlay Hint
- [x] used version in Cargo.lock

### Hover
- [x] available versions
- [x] available features
- [x] crate description(README)
- [ ] Static

### Code completion
- [ ] static manifest suggestions
- [ ] dependency
  - [x] name
    - [ ] filter existing
    - [ ] add workspace crates
    - [ ] sort
    - [ ] starts with, contains, starts_with_segment, treat - and _ the same
  - [x] dependency version
  - [x] dependency features
  - [x] dependency workspace
  - [ ] key when version after the key `crate = "0.1.0"` => `crate = {ve"0.1.0"` to `crate = { version = "0.1.0" }`
- [ ] features
  - [ ] local features `default = ["feature1", "feature2"]`
  - [ ] optional dependencies `dep:serde`
  - [ ] dependencies features `serde?/derive`

### Diagnostics
- [ ] Static format
- [x] Dependencies
  - [x] check if crate exists
  - [x] check if crate needs update
  - [x] check if crate version exists
  - [x] check if crate features exist
  - [x] check for feature duplicate
  - [x] check for dep duplicate
  - [x] check if version is set & dep in workspace
  - [ ] better target support
- [ ] Features
  - [ ] check for feature duplicate
  - [ ] check if `dep:crate_name` is optional

### Formatter
- [x] enable taplo formatter
- [ ] auto close { when content inside

## Plans
- feature suggestions for git dependencies and local dependencies
- use local readmes if available
- make cache persistent
