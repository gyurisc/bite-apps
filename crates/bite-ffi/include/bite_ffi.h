// C interface to the Bite engine. Declarations mirror crates/bite-ffi/src/lib.rs;
// keep the two in step by hand.

#ifndef BITE_FFI_H
#define BITE_FFI_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

// An engine instance. Use it from one thread at a time.
typedef struct BiteEngine BiteEngine;

// Borrowed UTF-8 bytes, not NUL-terminated. `ptr` may be NULL when `len` is 0.
typedef struct {
    const uint8_t *ptr;
    size_t len;
} BiteStr;

// One named command parameter.
typedef struct {
    BiteStr name;
    BiteStr value;
} BiteParam;

// The outcome of one command. Release it with bite_reply_free.
typedef struct {
    bool ok;
    bool can_undo;
    bool can_redo;
    // The affected document's ID when `ok`, otherwise 0.
    uint64_t document;
    // UTF-8: the document text when `ok`, otherwise an error message.
    uint8_t *text;
    size_t text_len;
} BiteReply;

// Creates an engine. Release it with bite_engine_free.
BiteEngine *bite_engine_new(void);

// Destroys an engine. A NULL `engine` is ignored.
void bite_engine_free(BiteEngine *engine);

// Runs the command `command` with `param_count` named parameters and reports
// its outcome. Bad input produces a failed reply. `params` may be NULL when
// `param_count` is 0.
BiteReply bite_engine_dispatch(BiteEngine *engine, BiteStr command,
                               const BiteParam *params, size_t param_count);

// Releases the text a reply owns. Call it exactly once per reply.
void bite_reply_free(BiteReply reply);

#endif
