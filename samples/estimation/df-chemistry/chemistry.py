# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License.
import json
import math
import numpy as np
import numpy.linalg as LA
import numpy.typing as npt
from qdk import qsharp
from argparse import ArgumentParser
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlparse
from urllib.request import urlretrieve


@dataclass
class FCIDumpFileContent:
    num_orbitals: int
    one_body_terms: list[tuple[list[int], float]]
    two_body_terms: list[tuple[list[int], float]]

    @classmethod
    def from_file(self, file_url):
        """Read FCIDump file (given by either a local path or a URL) and parse
        it to get the input data for double-factorized chemistry algorithm."""
        url = urlparse(file_url)

        file_name = ""
        if url.scheme in ["http", "https"]:
            # Download the file
            file_name = url.path.rsplit("/", 1)[-1]
            print(f"Downloading {file_url} to {file_name}...")
            urlretrieve(file_url, file_name)
        elif url.scheme in ["", "file"]:
            # Use the whole URL as the file path
            file_name = file_url
        else:
            raise f"Unsupported file location {file_url}"

        with open(file_name, "r") as f:
            lines = [str.strip() for str in f.readlines()]

        assert lines[0].startswith("&FCI")

        parse_header = True
        header = ""
        header_values = {}
        self.one_body_terms = []
        self.two_body_terms = []

        for line in lines:
            if parse_header:
                # File header might not have key-value pairs in separate lines,
                # so accumulate the whole header first and then parse.
                header += line + " "
                if line.endswith("&END"):
                    # Strip "&FCI" and "&END"
                    rest_header = header[4:-4]

                    # Find each key-value pair based on the "=" sign
                    while (ind := rest_header.find("=")) > -1:
                        key = rest_header[0:ind].strip()
                        rest_header = rest_header[ind + 1 :]
                        # Figure out which part of the rest is value: until
                        # either whitespace or (if key is not ORBSYM) comma
                        ind = rest_header.find("," if key != "ORBSYM" else " ")
                        value = rest_header[0:ind].strip()
                        rest_header = rest_header[ind + 1 :]
                        if key == "ORBSYM":
                            value = value[:-1]
                        header_values[key] = value
                    parse_header = False
            else:
                tokens = line.split()
                coefficient = float(tokens[0])
                indices = [int(str) - 1 for str in tokens[1:] if str != "0"]

                if len(indices) == 2:
                    indices.sort()
                    self.one_body_terms.append((indices, coefficient))
                elif len(indices) == 4:
                    # conversion from Mulliken to Dirac
                    i = indices[0]
                    j = indices[2]
                    k = indices[3]
                    l = indices[1]

                    symmetries = [
                        [i, j, k, l],
                        [j, i, l, k],
                        [k, l, i, j],
                        [l, k, j, i],
                        [i, k, j, l],
                        [k, i, l, j],
                        [j, l, i, k],
                        [l, j, k, i],
                    ]
                    symmetries.sort()
                    self.two_body_terms.append((symmetries[0], coefficient))

        self.num_orbitals = int(header_values["NORB"])

        return self


@dataclass
class TwoBodyResult:
    eigenvalues: list[npt.NDArray[np.float64]]
    eigenvectors: list[npt.NDArray[np.float64]]
    one_norms: list[float]
    two_norms: list[float]


@dataclass
class DoubleFactorization:
    num_orbitals: int
    rank: int
    one_body_norm: float
    two_body_norm: float
    one_body_eigenvalues: list[float]
    one_body_eigenvectors: npt.NDArray[npt.NDArray[np.float64]]
    two_body_eigenvalues: npt.NDArray[npt.NDArray[np.float64]]
    two_body_eigenvectors: list[npt.NDArray[npt.NDArray[np.float64]]]

    @classmethod
    def process_fcidump(self, structure: FCIDumpFileContent, error: float):

        # The `structure` provides us with one- and two electron integrals
        # h_{ij} and h_{ijkl} as described in Eq. (2).  These coefficients are
        # real and satisfy some symmetries which are outlined in Eq. (6).

        # In this step we compute R (`rank`) as in Eq. (7), h_{ij}
        # (`one_electron_vector`), and L_{ij}^{(r)} (`two_electron_vectors`).
        (rank, eigenvalue_signs, one_electron_vector, two_electron_vectors) = (
            self.perform_svd(structure)
        )

        # This computes Eq. (15).  It stores L^{-1}_{ij} into
        # `one_body_eigenvectors`, which is repurposed from
        # `one_electron_vector` and uses the same indices.  It also returns the
        # Schatten norm, which is the first summand in Eq. (16).
        (one_body_eigenvalues, one_body_eigenvectors, one_body_norm, norm2) = (
            self.process_one_body_term(
                structure.num_orbitals,
                rank,
                one_electron_vector,
                eigenvalue_signs,
                two_electron_vectors,
            )
        )

        assert len(one_body_eigenvectors) == structure.num_orbitals**2

        # This computes the second sum of Eq. (9), it computes the eigenvalues
        # \lambda_m^{(r)} and eigenvectors $\vec R_{m,i}^{(r)}$.  It does not
        # normalize the vectors, but returns the one- and two-norms of the
        # eigenvalues.
        two_body_result = self.process_two_body_terms(
            structure.num_orbitals, rank, two_electron_vectors
        )

        # Discard terms that will make the description exceed the error budget
        self.truncate_terms(structure.num_orbitals, error, two_body_result)

        # This computes the second summand in Eq. (16).
        two_body_norm = 0.0
        two_body_norm += sum(0.25 * norm * norm for norm in two_body_result.one_norms)

        # Reshape the one body eigenvectors so that they are represented
        # row-wise in a 2D array
        one_body_eigenvectors = np.reshape(
            one_body_eigenvectors, (structure.num_orbitals, structure.num_orbitals)
        )

        for i in range(len(two_body_result.eigenvectors)):
            cols = structure.num_orbitals
            rows = int(len(two_body_result.eigenvectors[i]) / cols)
            assert rows * cols == len(two_body_result.eigenvectors[i])

            # Reshape the two body eigenvectors so that they represented
            # row-wise in a 2D array
            two_body_result.eigenvectors[i] = np.reshape(
                two_body_result.eigenvectors[i], (rows, cols)
            )

        self.num_orbitals = structure.num_orbitals
        # Use the post-truncation rank
        self.rank = len(two_body_result.eigenvectors)
        self.one_body_norm = one_body_norm
        self.two_body_norm = two_body_norm
        self.one_body_eigenvalues = one_body_eigenvalues
        self.one_body_eigenvectors = one_body_eigenvectors
        self.two_body_eigenvalues = two_body_result.eigenvalues
        self.two_body_eigenvectors = two_body_result.eigenvectors

        return self

    def combined_index(i: int, j: int) -> int:
        return int(max(i, j) * (max(i, j) + 1) / 2 + min(i, j))

    def vectors_to_sym_mat(
        vector: npt.NDArray[float], dimension: int
    ) -> npt.NDArray[npt.NDArray[float]]:
        matrix = np.zeros((dimension, dimension), dtype=float)

        # Create lower triangular matrix
        matrix[np.tril_indices(dimension, 0)] = vector

        # Convert lower triangular matrix to symmetrix matrix
        matrix = matrix + matrix.T

        # Halve elements of diagonal to avoid doubling
        matrix = matrix - 0.5 * np.diag(np.diagonal(matrix, 0))

        return matrix

    @classmethod
    def populate_two_body_terms(
        self, two_body_terms: list[tuple[list[int], float]]
    ) -> list[tuple[list[int], float]]:
        complete_two_body_terms = []

        for [i, j, k, l], val in two_body_terms:
            ii = self.combined_index(i, l)
            jj = self.combined_index(j, k)

            complete_two_body_terms.append(([ii, jj], val))
            if ii != jj:
                complete_two_body_terms.append(([jj, ii], val))

        return complete_two_body_terms

    @classmethod
    def eigen_svd(
        self, orbitals: int, two_body_terms: list[tuple[list[int], float]]
    ) -> (int, list[int], list[float]):
        # combined = nC2 for n = orbitals
        combined = int(orbitals * (orbitals + 1) / 2)

        coeff_matrix = np.zeros((combined, combined), dtype=float)
        for [i, j], v in two_body_terms:
            coeff_matrix[int(i)][int(j)] = v

        # Compute the eigen decomposition of the symmetric coefficient matrix
        # with two body terms
        evals, evecs = LA.eigh(coeff_matrix)
        rows, cols = np.shape(evecs)

        evals_signs = np.zeros(cols, dtype=int)

        # let eigenvectors be represented as row vectors
        evecs = np.transpose(evecs)

        # Scale eigenvector by square root of corresponding eigenvalue
        for i in range(len(evals)):
            evecs[i] = math.sqrt(abs(evals[i])) * evecs[i]
            evals_signs[i] = np.sign(evals[i])

        # Collect eigenvectors as 1D array
        scaled_evecs_1D = np.reshape(evecs, cols * rows)

        return (cols, evals_signs.tolist(), scaled_evecs_1D)

    @classmethod
    def perform_svd(
        self, structure: FCIDumpFileContent
    ) -> (int, npt.NDArray[int], npt.NDArray[float], npt.NDArray[float]):

        full_two_body_terms = self.populate_two_body_terms(structure.two_body_terms)

        # Compute the eigen decomposition of two-electron terms
        (rank, eigenvalue_signs, two_electron_vectors) = self.eigen_svd(
            structure.num_orbitals, full_two_body_terms
        )

        length = (
            self.combined_index(structure.num_orbitals - 1, structure.num_orbitals - 1)
            + 1
        )
        one_electron_vector = np.zeros((length), dtype=float)

        # Collect one-electron terms into a single 2D array
        for [i, j], v in structure.one_body_terms:
            one_electron_vector[self.combined_index(i, j)] = v

        return (rank, eigenvalue_signs, one_electron_vector, two_electron_vectors)

    @classmethod
    def eigen_decomp(
        self, dimension: int, vector: npt.NDArray[float]
    ) -> (npt.NDArray[float], npt.NDArray[float], float, float):
        matrix = np.zeros((dimension, dimension), dtype=float)

        # Create lower triangular matrix
        matrix[np.tril_indices(dimension, 0)] = vector

        # Compute the eigen decomposition of the lower triangular matrix
        evals, evecs = LA.eigh(matrix, UPLO="L")
        norm1 = LA.norm(evals, 1)
        norm2 = LA.norm(evals)

        (rows, cols) = np.shape(evecs)
        evecs_1D = np.reshape(np.transpose(evecs), cols * rows)

        return (evals, evecs_1D, norm1, norm2)

    @classmethod
    def process_one_body_term(
        self,
        orbitals: int,
        rank: int,
        one_electron_vector: npt.NDArray[float],
        eigenvalue_signs: npt.NDArray[bool],
        two_electron_vectors: npt.NDArray[float],
    ) -> (list[float], npt.NDArray[float], float, float):
        # combined = nC2 for n = orbitals
        combined = int(orbitals * (orbitals + 1) / 2)

        vector = np.zeros(combined, dtype=float)

        for l in range(rank):
            matrix = np.zeros((orbitals, orbitals), dtype=float)
            H_l = self.vectors_to_sym_mat(
                two_electron_vectors[range(combined * l, combined * (l + 1))], orbitals
            )

            H_issj = eigenvalue_signs[l] * np.matmul(H_l, H_l)

            H_ssij = eigenvalue_signs[l] * np.trace(H_l) * H_l

            matrix = -0.5 * H_issj + H_ssij

            # Convert symmetric matrix to lower triangular matrix
            vector += matrix[np.tril_indices(orbitals, 0)]

        one_electron_vector += vector

        (one_body_eigenvalues, one_body_eigenvectors, one_body_norm, norm2) = (
            self.eigen_decomp(orbitals, one_electron_vector)
        )

        return (one_body_eigenvalues, one_body_eigenvectors, one_body_norm, norm2)

    @classmethod
    def process_two_body_terms(
        self, orbitals: int, rank: int, two_electron_vectors: npt.NDArray[float]
    ) -> TwoBodyResult:

        two_body_eigenvectors = []
        two_body_eigenvalues = []
        one_norms = []
        two_norms = []
        combined = int(orbitals * (orbitals + 1) / 2)

        for i in range(rank):
            matrix = two_electron_vectors[range(combined * i, combined * (i + 1))]
            (evals, evecs, norm1, norm2) = self.eigen_decomp(orbitals, matrix)

            two_body_eigenvalues.append(evals)
            two_body_eigenvectors.append(evecs)
            one_norms.append(norm1)
            two_norms.append(norm2)

        two_body_result = TwoBodyResult(
            two_body_eigenvalues, two_body_eigenvectors, one_norms, two_norms
        )

        return two_body_result

    @classmethod
    def truncate_terms(
        self, orbitals: int, error_eigenvalues: float, two_body_result: TwoBodyResult
    ):
        values_with_error = []
        for r, values in enumerate(two_body_result.eigenvalues):
            for i, v in enumerate(values):
                error = abs(v) * two_body_result.two_norms[r]
                values_with_error.append((error, r, i))

        # Sort in ascending order by error values
        values_with_error = sorted(values_with_error, key=lambda tup: tup[0])

        # Truncate the list so that the sum of squares of errors left is less
        # than the square of the input error
        total_error = 0
        truncate = len(values_with_error)
        for i, (error, _, _) in enumerate(values_with_error):
            error_compare = error**2
            if total_error + error_compare < error_eigenvalues**2:
                total_error += error_compare
            else:
                truncate = i
                break

        # Keep the first `truncate` values
        values_with_error = values_with_error[:truncate]

        indices_by_rank = []
        for _, r, i in values_with_error:
            while r >= len(indices_by_rank):
                indices_by_rank.append([])
            indices_by_rank[r].append(i)

        for r, indices in reversed(list(enumerate(indices_by_rank))):
            if len(indices) == orbitals:
                # All indices are to be removed: fully remove the r^th entry
                # for the norms, eigenvalues and eigenvectors.
                del two_body_result.eigenvalues[r]
                del two_body_result.eigenvectors[r]
                del two_body_result.one_norms[r]
                del two_body_result.two_norms[r]
            else:
                indices.sort()
                # Remove only eigenvalues/vectors corresponding to `indices`
                two_body_result.eigenvalues[r] = np.delete(
                    arr=two_body_result.eigenvalues[r], obj=indices
                )
                arr = np.reshape(two_body_result.eigenvectors[r], (orbitals, orbitals))
                arr = np.delete(arr=arr, obj=indices, axis=0)
                (rows, cols) = np.shape(arr)
                two_body_result.eigenvectors[r] = np.reshape(arr, rows * cols)

        # Check that same number of terms are removed for norms,
        # eigenvalues and eigenvectors
        assert (
            len(two_body_result.one_norms) == len(two_body_result.two_norms)
            and len(two_body_result.one_norms) == len(two_body_result.eigenvalues)
            and len(two_body_result.one_norms) == len(two_body_result.eigenvectors)
        )


# Convert 2D array into string representation
def ndarray2d_to_string(arr):
    str_arr = []
    for elem in arr:
        str_arr.append(np.array2string(elem, separator=","))
    return f"[{','.join(str_arr)}]"


# The script takes one required positional argument, URI of the FCIDUMP file
parser = ArgumentParser(description="Double-factorized chemistry sample")
# Use n2-10e-8o as the default sample.
# Pass a different filename to get estimates for different compounds
parser.add_argument(
    "-f",
    "--fcidumpfile",
    default="https://aka.ms/fcidump/n2-10e-8o",
    help="Path to the FCIDUMP file describing the Hamiltonian",
)
parser.add_argument(
    "-p",
    "--paramsfile",
    nargs="*",
    help="Optional parameter files to use for estimation",
)
args = parser.parse_args()

# ----- Read the FCIDUMP file and get resource estimates from Q# algorithm -----
structure = FCIDumpFileContent.from_file(args.fcidumpfile)
df = DoubleFactorization.process_fcidump(structure, 0.001)

# Load Q# project
this_dir = Path(__file__).parent
qsharp.init(project_root=this_dir)

# Construct the Q# operation call for which we need to perform resource estimate
str_one_body_eigenvalues = np.array2string(df.one_body_eigenvalues, separator=",")

str_one_body_eigenvectors = ndarray2d_to_string(df.one_body_eigenvectors)

str_two_body_eigenvalues = ndarray2d_to_string(df.two_body_eigenvalues)

str_two_body_eigenvectors = (
    "["
    + ",".join(
        [ndarray2d_to_string(eigenvectors) for eigenvectors in df.two_body_eigenvectors]
    )
    + "]"
)

qsharp_string = (
    "DoubleFactorizedChemistry.DoubleFactorizedChemistry("
    "DoubleFactorizedChemistry.DoubleFactorizedChemistryProblem("
    f"{df.num_orbitals}, {df.one_body_norm}, {df.two_body_norm}, "
    f"{str_one_body_eigenvalues}, {str_one_body_eigenvectors}, "
    f"[1.0, size = {df.rank}], {str_two_body_eigenvalues}, "
    f"{str_two_body_eigenvectors}),"
    "DoubleFactorizedChemistry.DoubleFactorizedChemistryParameters(0.001,))"
)

# Collect resource estimation parameters
if args.paramsfile is None:
    params = [
        {
            "errorBudget": 0.01,
            "qubitParams": {"name": "qubit_maj_ns_e6"},
            "qecScheme": {"name": "floquet_code"},
        }
    ]
else:
    params = []
    for paramsfile in args.paramsfile:
        with open(paramsfile) as f:
            data = json.load(f)
            if isinstance(data, dict):
                params.append(data)
            else:
                params += data

# Get resource estimates
res = qsharp.estimate(
    qsharp_string,
    params=params,
)

# Store estimates in json file
with open("resource_estimate.json", "w") as f:
    f.write(res.json)

result_obj = json.loads(res.json)
if isinstance(result_obj, dict):
    result_obj = [result_obj]

data = []
for item in result_obj:
    data_item = []

    # Run name
    data_item.append(item["jobParams"]["qubitParams"]["name"])

    # T factory fraction and Runtime
    if "physicalCountsFormatted" in item:
        data_item.append(
            item["physicalCountsFormatted"]["physicalQubitsForTfactoriesPercentage"]
        )
        data_item.append(item["physicalCountsFormatted"]["runtime"])
    elif "frontierEntries" in item:
        data_item.append(
            item["frontierEntries"][0]["physicalCountsFormatted"][
                "physicalQubitsForTfactoriesPercentage"
            ]
        )
        data_item.append(
            item["frontierEntries"][0]["physicalCountsFormatted"]["runtime"]
        )
    else:
        data_item.append("-")
        data_item.append("-")

    # Physical qubits and rQOPS
    if "physicalCounts" in item:
        data_item.append(item["physicalCounts"]["physicalQubits"])
        data_item.append(item["physicalCounts"]["rqops"])
    elif "frontierEntries" in item:
        data_item.append(item["frontierEntries"][0]["physicalCounts"]["physicalQubits"])
        data_item.append(item["frontierEntries"][0]["physicalCounts"]["rqops"])
    else:
        data_item.append("-")
        data_item.append("-")

    data.append(data_item)

# Define the table headers
headers = ["Run name", "T factory fraction", "Runtime", "Physical qubits", "rQOPS"]

# Determine the width of each column
col_widths = [max(len(str(item)) for item in column) for column in zip(headers, *data)]


# Function to format a row
def format_row(row):
    return " | ".join(
        f"{str(item).ljust(width)}" for item, width in zip(row, col_widths)
    )


# Create the table
header_row = format_row(headers)
separator_row = "-+-".join("-" * width for width in col_widths)
data_rows = [format_row(row) for row in data]

# Print the table
print(header_row)
print(separator_row)
for row in data_rows:
    print(row)

print("For more detailed resource counts, see file resource_estimate.json")
