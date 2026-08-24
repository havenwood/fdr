# Fdr

`Fdr` is a gem for `fd`-style file search and `rg`-style content search.

## Installation

```bash
gem install fdr
```

### Requirements

- Ruby 3.2+
- Rust 1.88+
- A C compiler and libclang

## Usage

### Search

`Fdr.search` returns a path-sorted `Array` of matching files and directories. `pattern` is a case-insensitive [Rust regular expression](https://docs.rs/regex) unless `glob: true`.

```ruby
require "fdr"

Fdr.search
#=> ["./Gemfile", "./LICENSE", "./README.md", "./Rakefile", "./ext", ...]
```

Keyword options are based on `fd` flags

```ruby
Fdr.search(extension: %w[rb rake])
Fdr.search(pattern: "*.{rb,rake}", glob: true)

Fdr.search(
  pattern: '\.test\.js$',
  paths: %w[src test],
  type: "f",
  exclude: %w[vendor],
  case_sensitive: true
)
```

### Grep

`Fdr.grep` returns a path-sorted `Hash` whose values map one-based line numbers to matching text. A line appears once regardless of how many times it matches.

```ruby
require "fdr"

Fdr.grep(pattern: "module Fdr", paths: %w[lib])
#=> {"lib/fdr.rb" => {13 => "module Fdr"}, "lib/fdr/version.rb" => {3 => "module Fdr"}}
```

`pattern` searches file contents. Filter files with `name` and the file-selection options from `Fdr.search`. `Fdr.grep` scans only regular files and does not accept `type`.

`pattern` is case-sensitive by default, unlike `name`, which follows `Fdr.search`. Pass `content_case_sensitive: false` for case-insensitive matching.

### Ignore files

By default, both `search` and `grep` respect `.ignore` and, inside a git repository, `.gitignore`, along with higher-precedence `.fdignore` for `Fdr.search` and `.rgignore` for `Fdr.grep`. `Fdr.search` also reads fd's global ignore file from `$XDG_CONFIG_HOME/fd/ignore` or `~/.config/fd/ignore`.

`ignore_file:` adds your own gitignore-format files at the lowest precedence. They still apply under `no_ignore: true`, like `fd` and `rg`.

### Behavior

Hidden files and directories are skipped unless `hidden: true` is passed.

Missing or unreadable entries are skipped. Pass `ignore_error: false` to raise `Fdr::IOError` on the first one instead. A bare `"-"` names the file `./-` rather than standard input.

`Fdr.search` and `Fdr.grep` release the GVL, support `Timeout` and `Ctrl-C`, and run independently in threads, Ractors and forks.

`Fdr` leaves out `fd`'s owner and exotic type filters and smart case, and `rg`'s context lines, literal and multiline matching, inverted matches and replacements.

## Attribution

`Fdr` incorporates code from `fd` under the MIT License.
