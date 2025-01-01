# Unreleased

# 0.8.0

- Update `tower-sessions` to 0.14.0

# 0.7.0

- Update `tower-sessions` to 0.13.0

# 0.6.1

- Update docs re web fundamentals
- Provide tracing for error cases
- Additional utility methods

# 0.6.0

- Update `tower-sessions` to 0.12.0

# 0.5.0

**Breaking Changes**

- Allow providing optional metadata for messages #8, #9

This change updates the `push` method to include an optional metadata argument; other methods are unchanged. A new set of `*_with_medata` postfixed methods is also provided.

# 0.4.0

- Update `tower-sessions` to 0.11.0

# 0.3.0

- Update `tower-sessions` to 0.10.0

# 0.2.2

- Implement `Display` for `Level`

# 0.2.1

- Save only when messages have been modified

# 0.2.0

**Breaking Changes**

- Rework crate into a middleware

This changes the structure of the crate such that it is now a middleware in addition to being an extractor. Doing so allows us to improve the ergonomics of the API such that calling `save` and awaiting a future is no longer needed.

Now applications will need to install the `MeessagesManagerLayer` after `tower-sessions` has been installed (either directly or via a middleware that wraps it).

Also note that the iterator impplementation has been updated to use `Message` directly. Fields of `Message` have been made public as well.

# 0.1.0

- Initial release :tada:
