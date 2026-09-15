# Install Liyasa

Everything the build needs is a supported platform and a terminal. Liyasa is a
single static binary with no runtime dependency on Node, a browser, or Docker.

## Supported platforms

Liyasa ships prebuilt binaries for Linux (x86-64 and aarch64, glibc and musl),
macOS (Apple silicon and Intel), and Windows (x86-64). The musl builds are
fully static and run on Alpine and on distroless images without a libc.

## Install the binary

The install script picks the right artifact for your platform, verifies its
checksum against the release manifest, and puts the binary on your path.

```sh
curl -fsSL https://kasecrab.github.io/liyasa/install.sh | sh
liyasa --version
```

If you would rather not pipe a script into a shell, download the archive for
your platform from the releases page, verify the checksum yourself, and move
the binary into a directory on your path.

```sh
tar -xzf liyasa-x86_64-unknown-linux-musl.tar.gz
sudo install -m 0755 liyasa /usr/local/bin/liyasa
```

## Install with a package manager

Homebrew, Scoop, and the Arch User Repository carry the same artifacts the
release publishes. Cargo builds from source with the default feature set,
which is the build and verification engines without the server.

```sh
brew install liyasa
scoop install liyasa
cargo install liyasa
```

## Run in a container

The published image is under 150 MB and runs as an unprivileged user. Mount
the project at `/docs` and the built site appears in `dist/`.

```sh
docker run --rm -v "$PWD:/docs" ghcr.io/kasecrab/liyasa:0.1 build
```

## Create a site

`liyasa new` writes a project you can build immediately: a `liyasa.json` with
the schema reference in place, a navigation tree, and three pages that
demonstrate front matter, components, and code samples.

```sh
liyasa new acme-docs
cd acme-docs
liyasa dev
```

The dev server watches the project and rebuilds what changed. A single page
edit is visible in under a hundred milliseconds on a project of a thousand
pages, because the build memoizes every query it makes and only the pages that
depend on the edited file are rendered again.

## Build for production

`liyasa build` writes the site to `dist/`: HTML, the Markdown twin of every
page, the search index, `llms.txt`, a sitemap, redirects, and the header files
that static hosts read.

```sh
liyasa build
liyasa test --agents
```

`liyasa test --agents` runs the agent-readiness checks against the built
output and prints a report per page: served bytes, converted characters, and
the ratio between them. A page that crosses a budget is a diagnostic with a
code, not a line of prose, so CI can fail on it.

## Verify the installation

`liyasa doctor` reports what is present and what is missing: the version, the
cache directory, the optional companion runtime that math, PDF export, and
Lighthouse budgets use, and whether a container runtime is available for code
verification.

```sh
liyasa doctor
```

Nothing in the list is required to build a site. A missing companion runtime
means math renders through the pure-Rust path and `liyasa export --pdf`
explains why it cannot run, not that the build fails.

## Next steps

Read [Configuration](/guide/configuration) for the keys that control theme,
navigation, and search, then [the CLI reference](/reference/cli) for the rest
of the commands.
