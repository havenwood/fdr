# Fdr

`Fdr` is a fast file-search gem for Ruby. Its Rust extension is derived from [fd](https://github.com/sharkdp/fd) and uses ripgrep's [ignore](https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore), [globset](https://github.com/BurntSushi/ripgrep/tree/master/crates/globset), [grep-regex](https://github.com/BurntSushi/ripgrep/tree/master/crates/regex) and [grep-searcher](https://github.com/BurntSushi/ripgrep/tree/master/crates/searcher) crates, plus [regex](https://github.com/rust-lang/regex).

`Fdr.search` covers most of `fd` and `Fdr.grep` just a small part of [ripgrep](https://github.com/BurntSushi/ripgrep). Use `Fdr` from Ruby and `fd` or `rg` from shell. `Fdr` doesn't have a CLI executable.

## Installation

```bash
gem install fdr
```

### Requirements
- Ruby 3.2+
- Rust

## Usage

```ruby
require 'fdr'

Fdr.search(extension: 'rb')

Fdr.search(
  pattern: 'test',
  paths: %w[lib spec],
  type: 'f'
)

Fdr.search(
  pattern: 'config',
  paths: %w[app config],
  extension: 'yml',
  type: 'f',
  max_depth: 3,
  hidden: true
)

Fdr.search(
  pattern: '\.test\.js$',
  paths: %w[src test],
  exclude: %w[node_modules vendor],
  case_sensitive: true
)

Fdr.search(pattern: '*.{rb,rake}', glob: true)

Fdr.search(
  extension: 'log',
  min_size: 1024 * 1024,
  changed_within: 86400,
  paths: %w[logs]
)

# Aliases for `Fdr.search`, if you prefer:
Fdr.entries(extension: 'rb')
Fdr.scan(extension: 'rb')
```

### Gaps

Missing: `fd`'s owner filters, executable/empty/socket/pipe/device types, smart case and `.fdignore`, plus `rg`'s context lines, fixed-string and multiline matching, inverted matches and replacements. No `--crlf` either, so `needle$` won't match before a CRLF.
