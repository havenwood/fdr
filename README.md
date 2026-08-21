# Fdr

`Fdr` is a Ruby gem for `fd`-style file search and `rg`-style content search.

## Installation

```bash
gem install fdr
```

Or add it to your `Gemfile`:

```ruby
gem 'fdr'
```

### Requirements

- Ruby 3.2+
- Rust 1.88+
- A C compiler and libclang

## Usage

`Fdr.search` returns path-sorted matches prefixed by each search root (`.` by default, producing `./...`). `pattern` is a case-insensitive [Rust regular expression](https://docs.rs/regex) unless `glob: true`.

Both honor `.gitignore` and `.ignore`, which use gitignore syntax and `no_ignore: true` disables.

Sizes are bytes and times are seconds ago, for `min_size`, `max_size`, `changed_within` and `changed_before`.

```ruby
require 'fdr'

Fdr.search
Fdr.search(extension: %w[rb rake])
Fdr.search(pattern: '*.{rb,rake}', glob: true)

Fdr.search(
  pattern: '\.test\.js$',
  paths: %w[src test],
  type: 'f',
  exclude: %w[vendor],
  case_sensitive: true
)
```

### Grep

`Fdr.grep` returns a path-sorted `Hash` whose values map one-based line numbers to matching text. A line appears once regardless of how many times it matches.

```ruby
Fdr.grep(pattern: 'module Fdr', paths: %w[lib])
#=> {"lib/fdr.rb" => {13 => "module Fdr"}, "lib/fdr/version.rb" => {3 => "module Fdr"}}
```

`pattern` searches file contents. Filter files with `name` and the file-selection options from `Fdr.search`. `Fdr.grep` scans only regular files and does not accept `type`.

```ruby
Fdr.grep(
  pattern: 'TODO',
  name: '_spec\.rb$',
  paths: %w[spec],
  extension: 'rb',
  max_depth: 3,
  hidden: true
)
```

`pattern` is case-sensitive by default, unlike `name`, which follows `Fdr.search`. Pass `content_case_sensitive: false` for case-insensitive matching.

### Behavior

Hidden files and directories are skipped unless `hidden: true` is passed.

Searches release the GVL, support `Timeout` and `Ctrl-C`, and run independently in threads and forks.

`Fdr` covers the common ground. It leaves out `fd`'s owner and exotic type filters and smart case, and `rg`'s context lines, literal and multiline matching, inverted matches and replacements.

## Attribution

`Fdr` incorporates code from `fd` under the MIT License.
