<h2 style="color:#D30982;">Typed objects and data flow</h2>

Every step in the Chemistry QDK pipeline produces and consumes strongly-typed objects. Understanding this map prevents the most common errors — passing an `Orbitals` object where a `Wavefunction` is expected, or forgetting to call `.get_orbitals()` before passing to a Hamiltonian constructor. The diagram below walks you through the most important component of the overall Microsoft Chemistry QDK. 

<img src="https://storage.googleapis.com/qbraid-articles-staging/q-course-microsoft-chemistry-qdk/ch1-quantum_workflow_19.jpg?X-Goog-Algorithm=GOOG4-RSA-SHA256&X-Goog-Credential=cloud-run-api%40qbraid-staging.iam.gserviceaccount.com%2F20260505%2Fauto%2Fstorage%2Fgoog4_request&X-Goog-Date=20260505T205515Z&X-Goog-Expires=3600&X-Goog-SignedHeaders=host&X-Goog-Signature=REDACTED">

