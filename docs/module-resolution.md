# Module Resolution

> How Coco resolves `import` specifier strings to source files.

---

## Import syntax

Coco imports use ES-style syntax with named or namespace bindings:

```coco
import { Server, Response } from "std/http";
import { User } from "./models/user";
import * as crypto from "std/crypto";
```

| Form | Syntax | Example | Binding |
|---|---|---|---|
| Named | `import { A, B } from "path"` | `import { readFile } from "std/fs"` | `readFile` in scope |
| Namespace | `import * as name from "path"` | `import * as fs from "std/fs"` | `fs.readFile(...)` |
| Default | Not supported in Coco | — | — |

## Specifier kinds

Specifiers fall into three categories, resolved in order:

### 1. Standard library (`"std/..."` prefix)

Specifiers starting with `std/` resolve to the built-in standard library:

```coco
import { Server } from "std/http";
import { readFile } from "std/fs";
import { sleep } from "std/time";
```

Resolution: the compiler maps `std/<module>` to the corresponding stdlib module. No filesystem lookup — the stdlib is embedded in the runtime.

### 2. Relative paths (`"./"` or `"../"` prefix)

Specifiers starting with `./` or `../` are resolved relative to the importing file's directory:

```coco
// File: src/services/user-service.co
import { User } from "../models/user";       // → src/models/user.co
import * as validate from "../utils/validate"; // → src/utils/validate.co
import { db } from "./database";              // → src/services/database.co
```

Resolution rules:
1. Start from the directory containing the importing file
2. Resolve `..` and `.` segments
3. Append `.co` extension if not present
4. If the result is a directory, look for `index.co` inside it
5. Error if no file found

### 3. Package imports (no prefix or other prefix)

Specifiers without `std/`, `./`, or `../` are package imports. These resolve from installed dependencies:

```coco
import { something } from "third-party-lib";
import { helper } from "my-org/my-package";
```

Resolution: looks in `vendor/` or a registered package source. Package resolution is deferred to the package manager (Phase 10).

## File extension rules

- `.co` is the standard Coco source extension
- Extension may be omitted in import specifiers — the compiler appends `.co` automatically
- Explicit `.co` extension is also accepted

```coco
import { User } from "./models/user";    // resolves to ./models/user.co
import { User } from "./models/user.co"; // same result
```

## Directory imports

When a specifier resolves to a directory, the compiler looks for an `index.co` file:

```coco
import * as utils from "./utils"; // → ./utils/index.co
```

## Resolution order (summary)

For an import in file `src/app.co`:

| Specifier | Category | Resolves to |
|---|---|---|
| `"std/http"` | Stdlib | `std/http` (built-in) |
| `"./config"` | Relative | `src/config.co` |
| `"../lib/util"` | Relative | `lib/util.co` |
| `"./utils/"` | Dir import | `src/utils/index.co` |
| `"some-package"` | Package | Deferred to pkg manager |

## Export syntax

Coco uses `export` to make items available to importers:

```coco
// Named exports:
export class User { ... }
export type UserId = int;
export fn createUser(name: string): User { ... }

// Re-export:
export { User } from "./models/user";
```

Items without `export` are private to the module and cannot be imported by other files.

## Module scope

- Each `.co` file is a module
- The filename (minus extension) is the module name
- All top-level declarations are module-scoped
- `export` makes a declaration visible to importers
- No circular dependency detection in early phases — this is a Phase 7+ concern
