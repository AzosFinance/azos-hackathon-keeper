# Azos Keeper

This is a multi-language Keeper implementation for the Azos project, implemented in:

- Rust
- JavaScript

To view the appropriate implementation's documentation, navigate to the README for the appropriate code folder.

<img width="1097" alt="image" src="https://github.com/AzosFinance/azos-keeper/assets/10622322/234b187e-0f63-4680-a2de-39d77be83424">

## Context

As the Azos platform goes about its operations, the stablecoin may change in trading value against other coins. To counter this, the Keeper will observe the trading values and take appropriate action if the value is too high or too low. If the value becomes too low, the `buyAndContract` function will be called. Conversely, if too high, the `sellAndExpand` function will be called.
