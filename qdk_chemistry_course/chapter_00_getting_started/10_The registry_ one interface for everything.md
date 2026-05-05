<h2 style="color:#D30982;">The registry: one interface for everything</h2>

Every algorithm in `qdk-chemistry` — SCF solvers, active space selectors, qubit mappers, QPE — is created through a single registry. There are no scattered constructors or module-specific imports to remember. The pattern is always:

```
from qdk_chemistry.algorithms import available, create, inspect_settings

create(algorithm_type, algorithm_name, **settings)
```

This design means you can swap implementations (e.g. different active space selectors) by changing one string, and discover what's available without reading source code.