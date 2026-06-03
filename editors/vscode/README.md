# Coco Language — VS Code Extension

Syntax highlighting, snippets, and language configuration for `.co` files.

## Build VSIX

Requires Node.js.

```bash
cd editors/vscode
npx @vscode/vsce package --out ../../coco-lang-0.1.0.vsix
```

This produces `coco-lang-0.1.0.vsix` in the repo root.

## Install

### From VSIX (recommended)

```bash
code --install-extension coco-lang-0.1.0.vsix
```

Or in VS Code: `Ctrl+Shift+P` → "Extensions: Install from VSIX..." → select the `.vsix` file.

### From source (symlink)

```bash
# Linux/macOS:
ln -s $(pwd)/editors/vscode ~/.vscode/extensions/coco-lang

# Windows (PowerShell as admin):
New-Item -ItemType Junction -Path "$env:USERPROFILE\.vscode\extensions\coco-lang" -Target "editors\vscode"
```

### After Install

Restart VS Code. Files with `.co` extension will get syntax highlighting automatically.

## Features

### Syntax Highlighting

- Keywords, control flow, declarations
- String literals and template strings with `${expr}` interpolation
- Numeric literals (decimal, hex, binary, octal)
- Type annotations and generics
- Magic methods (`__toString`, `__get`, etc.)
- Operators including `<=>`, `?.`, `??`, `?:`
- Comments (line and block)

### Snippets

| Prefix | Description |
|--------|-------------|
| `fn` | Named function |
| `afn` | Async function |
| `arrow` | Arrow function |
| `class` | Class with constructor |
| `trait` | Trait with state |
| `enum` | Enum |
| `match` | Match expression |
| `if` / `ife` | If / If-else |
| `forin` | For-in loop |
| `try` / `tryf` | Try-catch / Try-catch-finally |
| `ok` / `err` | Return Ok/Err |
| `parallel` | Parallel block |
| `chan` | Channel creation |
| `select` | Select statement |
| `coro` | Coroutine |
| `import` | Import statement |
| `main` / `amain` | Main / Async main |
| `httpserver` | HTTP server boilerplate |
| `ctor` | Constructor |
| `__tostring` | Magic toString |
| `__compare` | Magic compare |

### Language Configuration

- Auto-closing pairs for brackets, quotes, backticks
- Comment toggling (line: `//`, block: `/* */`)
- Bracket matching and colorization
- Folding support
- Indentation rules
