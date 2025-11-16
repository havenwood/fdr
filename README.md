# Fdr

`Fdr` is a fast file search gem for Ruby, implemented with a Rust native extension inspired directly by [fd](https://github.com/sharkdp/fd). `Fdr` uses the same core dependencies as `fd` and `ripgrep`, including [ignore](https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore), [regex](https://github.com/rust-lang/regex), [globset](https://github.com/BurntSushi/ripgrep/tree/master/crates/globset) and [crossbeam-channel](https://github.com/crossbeam-rs/crossbeam).

`Fdr` intentionally lacks an `fdr` executable, since `fd` is perfect for that job. If you need a fast file searching in a CLI tool use, use `fd`. If you need fast file searching from your Ruby code, use `Fdr`.

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

Fdr.search(pattern: '**/*.{rb,rake}', glob: true)

Fdr.search(
  extension: 'log',
  min_size: 1024 * 1024,
  changed_within: 86400,
  paths: %w[logs]
)

Fdr.search(
  pattern: 'thought.*snow|garret.*auction|foul.*thing',
  paths: %w[~/garret ~/vault],
  extension: 'txt',
  type: 'f',
  hidden: true,
  no_ignore: true,
  case_sensitive: false,
  glob: false,
  full_path: true,
  max_depth: 7,
  min_depth: 1,
  exclude: %w[publication creator],
  follow: true,
  min_size: 1,
  max_size: 1_048_576,
  changed_within: 31_536_000,
  changed_before: 604_800
)

# Aliases for `Fdr.search`, if you prefer:
Fdr.entries(extension: 'rb')
Fdr.scan(extension: 'rb')
```

### Gaps

Some non-CLI features that `fd` implements that are lacking `Fdr` including owner filters, support for nonfile types, smart case switching with patterns and `.fdignore` support.
