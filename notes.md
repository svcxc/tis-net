# On behavior of the `ANY` port

In TIS-100, the `ANY` port as source register, when contested, prioritizes in the following order:

- UP
- LEFT
- RIGHT
- DOWN

When used as the destination register, it prioritizes in the following order:

- LEFT
- RIGHT
- UP
- DOWN

# On behavior of stack nodes

- Stack nodes can be written from multiple nodes in a single cycle. The writes will then happen in the same order as the `ANY` port's resolution order (probably, not 100% verifiable I think, but I don't see any reason why it would work differently).
- Stack nodes cannot be read from by multiple nodes in a single cycle.
- Stack nodes can be written to and then read from in a single cycle; the read will happen before the write.

# Existing documented implementation of ANY port solving

https://github.com/T045T/TIS-100