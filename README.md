# cargotom
## Config
```json
"lsp": {
  "cargo-tom": {
    "initialization_options": {
      "hide_docs_info_message": true,
      "offline": false,
      "stable": false,
      "daemon": true,
      "per_page_web": 50
    }
  },
  ...
}
```
## Features
### Code actions
- [x] Open LSP docs(first line)
- [x] Open LSP issues(first line)
- [x] "Make Workspace dependency" => This will generate `{ workspace = true }` for the dependency
- [x] "Expand dependency specification" => This will convert from `"0.1.0"` to `{ version = "0.1.0" }`
- [x] "Collapse dependency specification" => This will convert from `{ version = "0.1.0" }` to `"0.1.0`
- [x] "Open Docs" => opens docs.rs/...
- [x] "Open crates.io" => opens crates.io/...
- [x] "Upgrade" => will upgrade the dependency version to the latest version
- [x] "Upgrade All " => will upgrade every dependency version to the latest version
- [x] "Update All" => will run `cargo run`
- [ ] toggle optional dependency

### Inlay Hint
- [x] used version in Cargo.lock

### Hover
- [x] available versions
- [x] available features
- [ ] crate description

### Code completion
- [ ] static manifest suggestions

#### Dependencies
- [x] crate names(online/offline)
- [ ] crate versions(online/offline)
  - [x] latest version
  - [x] all version suggestions
  - [ ] workspace = true if in workspace
- [ ] key when version after the key `crate = "0.1.0"` => `crate = {ve"0.1.0"` to `crate = { version = "0.1.0" }`

#### Features
- [x] crate features(online/offline)
  - [x] local features `default = ["feature1", "feature2"]`
  - [x] optional dependencies `dep:serde`
  - [x] dependencies features `serde?/derive`

### Diagnostics
- [ ] check if crate needs update
- [ ] check if crate version exists
- [ ] check if crate features exist
- [ ] check if optional dependencies are used(features)
- [ ] check if version is set & dep in workspace
- [ ] check for feature duplicate
- [ ] check for dep duplicate
- [ ] check if `dep:crate_name` is optional

### Formattwer
- [ ] enable taplo formatter
- [ ] auto close { when content inside

## Plans
- resolve workspace = true to version
