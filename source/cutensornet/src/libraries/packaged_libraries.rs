// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`PackagedLibraries`]: the library files a package manager installed,
//! which a caller gives discovery to try before the default paths.

use std::path::PathBuf;

/// The library files a package manager installed, for
/// [`discover_with`](crate::discover_with), and the command that installs
/// them.
///
/// A package manager can install the libraries where no default path
/// reaches. NVIDIA's Python wheels (`cutensornet-cu12`,
/// `nvidia-cuda-runtime-cu12`) put them inside a Python environment, which
/// only a caller that reads the environment can find. For each library,
/// the first file of its file name (`libcutensornet.so.2`,
/// `libcudart.so.12`) is tried before the default paths. A later file of
/// that name, or a file of another name, is not tried. `Default` is no file
/// and no command, which is what [`discover`](crate::discover) uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackagedLibraries {
    /// The absolute paths of the library files installed. Order matters
    /// only between files of one name.
    pub files: Vec<PathBuf>,
    /// The command that installs the libraries with the caller's package
    /// manager, or `None`. The setup check's advice gives it as an example.
    pub install_command: Option<String>,
}
