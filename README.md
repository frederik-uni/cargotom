# cargotom
## Config
```json
"lsp": {
  "cargo-tom": {
    "initialization_options": {
      "hide_docs_info_message": true,
      "offline": false,
      "stable": false,
      "sort": false,
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
- [x] Open Cargo.toml docs(first line)
- [x] "Make Workspace dependency" => This will generate `{ workspace = true }` for the dependency
- [x] "Expand dependency specification" => This will convert from `"0.1.0"` to `{ version = "0.1.0" }`
- [x] "Collapse dependency specification" => This will convert from `{ version = "0.1.0" }` to `"0.1.0`
- [x] "Open Docs" => opens docs.rs/...
- [x] "Open crates.io" => opens crates.io/...
- [x] "Open Src code" => opens src code on github
- [x] "Upgrade" => will upgrade the dependency version to the latest version
- [x] "Upgrade All" => will upgrade every dependency version to the latest version
- [ ] "Update All" => will run `cargo run`
- [x] toggle optional dependency
- [ ] make dependency optional if in feature


### Inlay Hint
- [ ] used version in Cargo.lock

### Hover
- [ ] available versions
- [ ] available features
- [ ] crate description
- [ ] Static

### Code completion
- [ ] static manifest suggestions
- [ ] dependency
  - [ ] name
  - [ ] dependency version
  - [ ] dependency features
  - [ ] dependency workspace
  - [ ] key when version after the key `crate = "0.1.0"` => `crate = {ve"0.1.0"` to `crate = { version = "0.1.0" }`
- [ ] features
  - [ ] local features `default = ["feature1", "feature2"]`
  - [ ] optional dependencies `dep:serde`
  - [ ] dependencies features `serde?/derive`

### Diagnostics
- [ ] check if crate needs update
- [ ] check if crate version exists
- [ ] check if crate features exist
- [ ] check if optional dependencies are used(features)
- [ ] check if version is set & dep in workspace
- [ ] check for feature duplicate
- [ ] check for dep duplicate
- [ ] check if `dep:crate_name` is optional

### Formatter
- [ ] enable taplo formatter
- [ ] auto close { when content inside

## Plans
- resolve workspace = true to version
- feature suggestions for git dependencies and local dependencies
