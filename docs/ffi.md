# FFI — Foreign Function Interface

> How Coco code interacts with native libraries through the Foreign Function Interface.

---

## Overview

Coco provides an FFI layer for calling native code (C libraries, system APIs) from Coco. All FFI calls must occur inside `unsafe { }` blocks. The FFI is an explicit escape hatch — not a primary API.

## The `ffi` module

FFI functions live under a built-in `ffi` namespace:

```coco
import { ffi } from "std/ffi";
```

## Loading a native library

```coco
import { ffi } from "std/ffi";

const lib = ffi.load("libsqlite3"); // resolves .so/.dylib/.dll by platform
```

`ffi.load(name)` finds the library using the platform's standard search path (`LD_LIBRARY_PATH`, `DYLD_LIBRARY_PATH`, `PATH`).

Explicit paths are also supported:

```coco
const lib = ffi.load("./vendor/libcustom.so");
```

## Defining native functions

Each native function is declared with its signature before use:

```coco
const sqlite3_open = lib.fn("sqlite3_open", [ffi.CString, ffi.Pointer], ffi.Int32);
const sqlite3_exec = lib.fn("sqlite3_exec", [ffi.Pointer, ffi.CString, ffi.Pointer, ffi.Pointer, ffi.Pointer], ffi.Int32);
const sqlite3_close = lib.fn("sqlite3_close", [ffi.Pointer], ffi.Int32);
```

### FFI types

| Coco FFI type | C equivalent | Size (bytes) |
|---|---|---|
| `ffi.Int8` | `int8_t` | 1 |
| `ffi.Int16` | `int16_t` | 2 |
| `ffi.Int32` | `int32_t` | 4 |
| `ffi.Int64` | `int64_t` | 8 |
| `ffi.UInt8` | `uint8_t` | 1 |
| `ffi.UInt16` | `uint16_t` | 2 |
| `ffi.UInt32` | `uint32_t` | 4 |
| `ffi.UInt64` | `uint64_t` | 8 |
| `ffi.Float32` | `float` | 4 |
| `ffi.Float64` | `double` | 8 |
| `ffi.CString` | `const char *` | pointer |
| `ffi.Pointer` | `void *` | pointer |
| `ffi.Void` | `void` (return only) | 0 |
| `ffi.Bool` | `bool` (C99 `_Bool`) | 1 |
| `ffi.SizeT` | `size_t` | platform |

### Calling native functions

All FFI calls must be wrapped in `unsafe { }`:

```coco
unsafe {
    const db = ffi.alloc(ffi.Pointer);  // allocate a pointer-sized buffer
    const rc = sqlite3_open(":memory:", db);

    if rc != 0 {
        throw new Error(`sqlite3_open failed: ${rc}`);
    }

    const dbPtr = ffi.readPointer(db, 0);
    sqlite3_close(dbPtr);
    ffi.free(db);
}
```

## Memory management in FFI

Coco does not manage native memory. The developer is responsible for:

| Operation | Coco FFI function |
|---|---|
| Allocate | `ffi.alloc(type, count?)` |
| Free | `ffi.free(ptr)` |
| Read value | `ffi.readInt32(ptr, offset?)`, `ffi.readFloat64(ptr, offset?)` |
| Write value | `ffi.writeInt32(ptr, value, offset?)` |
| Read pointer | `ffi.readPointer(ptr, offset?)` |
| Read C string | `ffi.readCString(ptr)` |
| Create C string | `ffi.createCString(cocoStr)` |
| Copy memory | `ffi.memcpy(dst, src, size)` |

Memory allocated with `ffi.alloc` is NOT garbage collected. It must be freed with `ffi.free`.

## Callbacks

Coco functions can be passed as C callbacks:

```coco
const callback = ffi.callback([ffi.Pointer, ffi.Int32, ffi.Pointer], ffi.Int32, (data, cols, values) => {
    // process row
    return 0;
});

unsafe {
    sqlite3_exec(dbPtr, "SELECT * FROM users", callback, ffi.null, ffi.null);
}
```

`ffi.callback(paramTypes, returnType, fn)` creates a C-callable function pointer from a Coco closure.

## Safety modes and FFI

| Safety mode | FFI behavior |
|---|---|
| `application` | FFI usage requires `unsafe` + explicit allowlist in `coco.toml` |
| `library` | FFI allowed inside `unsafe` blocks |
| `systems` | FFI allowed; `unsafe` block optional for FFI calls |

### Application mode allowlisting

```toml
# coco.toml
[safety]
mode = "application"
allow_unsafe_dependencies = false

[safety.allow]
ffi = "sqlite3,libcurl"    # only allowlisted libraries can be loaded
```

## Structs and complex types

C struct support is deferred to a post-v1 phase. For now, use `ffi.Pointer` with manual offset calculations and `ffi.memcpy` for struct field access.

## Error handling

- `ffi.load` throws if the library cannot be found
- `lib.fn` throws if the function is not in the library
- Native calls that segfault or access invalid memory cause an immediate process abort (no recovery — that's why it's `unsafe`)

## What's deferred

- Struct layout definitions and automatic field access
- Union types
- Variadic functions (`printf`-style)
- C++ class interop
- Inline assembly
- Automatic string conversion (CString ↔ coco string)
