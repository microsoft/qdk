<h2 style="color:#D30982;">Setup: local environment</h2>

Complete these steps before running any code in this course. This is the only chapter that requires local setup — all other chapters run inside your configured environment.

<h3>Step 1: Install VS Code</h3>

1. Download and install <a href="https://code.visualstudio.com/" target="_blank">Visual Studio Code</a>
2. Open VS Code and install the <strong>Microsoft Quantum Development Kit</strong> extension 

<h3>Step 2: Install <code>qdk-chemistry</code></h3>
In a terminal (inside your virtual environment):

```
pip install 'qdk-chemistry[plugins, qiskit-extras, jupyter]'
```

Full installation instructions including system dependencies (LAPACK, HDF5): <a href="https://github.com/microsoft/qdk-chemistry/blob/main/INSTALL.md" target="_blank">INSTALL.md</a>

<h3>Step 3: Clone the course repository</h3>
In a terminal:

```
git clone https://github.com/qBraid/microsoft-course.git
cd microsoft-course
```

This gives you all the course notebooks (with exercises) and the example data files used throughout.


<h3>Step 4: Open the course in VS Code</h3>

1. In VS Code, open the cloned <code>microsoft-course</code> folder
2. Open <code>chemistry/course/chapter_00_getting_started.ipynb</code>
3. Select your Python environment as the kernel
4. Run the verification cell below — it should print the installed version without errors

If you are also taking the QDK course, its notebooks are in <code>qdk/course/</code> — both courses live in the same repository.