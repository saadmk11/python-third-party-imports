# python-third-party-imports

This is a **Python** **CLI** tool built with **Rust** that finds all third-party packages imported into your Python project.

# Install

You can install this package via pip.

```console
pip install third-party-imports
```

# Usage

### Run:

```console
third-party-imports path/to/project/dir
```

### Help:

```console
third-party-imports --help

Find all third-party packages imported into your python project.

Usage: third-party-imports <PROJECT_ROOT>

Arguments:
  <PROJECT_ROOT>  Path to the project's root directory

Options:
  -h, --help     Print help
  -V, --version  Print version
```

# Example

```console
third-party-imports examples/
```

**Output:**

```console
Found '4' third-party package imports in '5' files. (Took 920.50µs)

celery
django
pandas
requests
```

# Development

### Run using Cargo

```console
cargo +nightly run -- path/to/project/dir
```

### Code Format

```console
cargo +nightly fmt
```

### Run Tests

```console
cargo +nightly test
```

### Install Package in current `virtualenv`

```console
maturin develop
```

# License

The code in this project is released under the [MIT License](LICENSE).
