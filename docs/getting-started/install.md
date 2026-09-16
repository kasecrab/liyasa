---
title: Install
description: Liyasa is one static binary. Install it with a script, a package manager, a container image, or from source.
---

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

::::code-group

```sh {title="Homebrew"}
brew install liyasa
```

```sh {title="Scoop"}
scoop install liyasa
```

```sh {title="Cargo"}
cargo install liyasa
```

::::

## Run in a container

The published image runs as an unprivileged user. Mount the project at `/docs`
and the built site appears in `dist/`.

```sh
docker run --rm -v "$PWD:/docs" ghcr.io/kasecrab/liyasa:0.1 build
```

## The companion runtime

A few features genuinely need a JavaScript engine or a browser, and those live
behind an optional **companion runtime** rather than in the binary: rendering
maths with KaTeX, pre-rendering Mermaid diagrams to static SVG, exporting PDF,
running accessibility tests in a real browser, Lighthouse budgets, and
screenshot sources for verification.

```sh
liyasa companion install
```

:::note{title="Nothing here is required to build a site"}
Without the companion, maths renders through the pure-Rust path, Mermaid
diagrams render in the browser from the fence source, and `liyasa export --pdf`
explains why it cannot run. The build does not fail.
:::

## Verify the installation

`liyasa doctor` reports what is present and what is missing: the version, the
cache directory, the companion runtime, and whether a container runtime is
available for code verification.

```sh
liyasa doctor
```

If a check fails, its output carries an error code, and every code has a page
in [the error reference](/errors). A missing sandbox is
[`E0004`](/errors/E0004); a missing companion runtime is
[`E0003`](/errors/E0003).

## Next steps

[The quickstart](/getting-started/quickstart) builds a site from nothing in
about a minute. [Project layout](/getting-started/project-layout) explains what
each directory in a Liyasa project is for.
