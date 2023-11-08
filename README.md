# Azos Keeper

This is a multi-language Keeper implementation for the Azos project, implemented in:

- Rust
- JavaScript

To view the appropriate implementation's documentation, navigate to the README for the appropriate code folder.

## Context

As the Azos platform goes about its operations, the stablecoin may change in trading value against other coins. To counter this, the Keeper will observe the trading values and take appropriate action if the value is too high or too low. If the value becomes too low, the `buyAndContract` function will be called. Conversely, if too high, the `sellAndExpand` function will be called.
