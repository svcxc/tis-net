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