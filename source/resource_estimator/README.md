# Microsoft Quantum Resource Estimator

The implementation for the Resource Estimator is broken up into two major components:

- [counts](./src/counts.rs) - performs program execution tracing to capture the logical qubit and gate counts for the given program
- [estimates](./src/estimates.rs) - takes in logical counts and a configuration to produce the set of corresponding physical resource estimates

For more information about the Microsoft Quantum Resource Estimator, see [the official documentation](https://learn.microsoft.com/en-us/azure/quantum/intro-to-resource-estimation).

The theoretical models used in Microsoft Quantum Resource Estimator are described in [Beverland at al. "Assessing requirements to scale to practical quantum advantage"](https://arxiv.org/abs/2211.07629).

## Using the resource estimator crate for customizable tasks

In the [examples](./examples/) directory we show how to perform custom resource estimation tasks, by directly using the crate. Note however, that we do not guarantee that there are no breaking changes even among minor releases.
We advise to specify the dependency location to the resource estimator crate by pointing to a specific release tag or commit hash (see [The Cargo Book](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#specifying-dependencies-from-git-repositories) for more details):

```toml
resource_estimator = { git = "https://github.com/microsoft/qsharp.git", tag = "<tag>" }
# ...
resource_estimator = { git = "https://github.com/microsoft/qsharp.git", rev = "<commit_hash>" }
```
