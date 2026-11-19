// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`PackagedLibraries`]: the library files a package manager installed,
//! which a caller gives discovery to try before the default paths.

use std::path::PathBuf;

/// The library files a package manager installed, for
/// [`discover_with`](crate::discover_with), and the command that installs
/// them.
///
/// A package manager may install the libraries where no default path
/// reaches: NVIDIA's Python wheels (`cutensornet-cu12`,
/// `nvidia-cuda-runtime-cu12`) put them inside a Python environment, which
/// only a caller that reads the environment can find. The first file of a
/// library's file name (`libcutensornet.so.2`, `libcudart.so.12`) is tried
/// for that library, before the default paths; a later file of that name,
/// or a file of another name, is not tried. `Default` is no file and no
/// command, which is what [`discover`](crate::discover) uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackagedLibraries {
    /// The absolute paths of the library files installed; order matters
    /// only between files of one name.
    pub files: Vec<PathBuf>,
    /// The command that installs the libraries with the caller's package
    /// manager, which the setup check's advice gives as an example; `None`
    /// gives no command.
    pub install_command: Option<String>,
}
