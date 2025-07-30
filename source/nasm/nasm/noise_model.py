import os
import json
import numpy as np
from numpy import ndarray


class NoiseModel:
    """
    Represents a noise model used for noisy simulation. This will be written to
    disk as a JSON file with a format similar to the following (replace and repeat
    'n_1q', 'xx', 'Mz', etc. as appropriate)
    {
        "krausOperators": {
            "n_1q": [ ["___", "___"], ["___", "___"] ],
            "n_2q": [ ["___", "___"] ],
        },
        "gates": {
            "xx": ( ["___", "___"], "n_2q" ),
            "zz": ( ["___", "___"], "n_2q" ),
        },
        "instruments": {
            "Mz": [ (["___", "___"], "n_1q", "0"), (["___", "___"], "n_1q", "1") ]
        },
        "models": {
            "": ["xx", "zz", "Mz"]
        }
    }

    An "___" is a string representation of a complex array, e.g. "[1.+0.j, 0.+0.j]"
    Quotes are needed as JavaScript/JSON does not support complex numbers natively.

    The "" model is considered default, but alternate models may be provided. This
    allows for multiple noise model configurations to be provided in one config file.

    Noise should be provided WITHOUT the operator included (i.e. noise only models).
    This allows the same noise model (e.g. depolarize by 5%) to be applied to multiple
    operators.
    """

    # Sets of Kraus matrices representing only noise component indexed by noise name
    kraus_operators: dict[str, list[ndarray]]

    # Gates indexed by gate name. Each entry contains gate operator and noise name
    gates: dict[str, tuple[ndarray, str]]

    # Measurement gates indexed by gate name. Each entry contains the list of: gate operator, noise name and outcome string
    instruments: dict[str, list[tuple[ndarray, str, str]]]

    # Lists indexed by noise model name. Each List contains gate and instrument names
    noise_models: dict[str, list[str]]

    # Default model name, which typically is "" (an empty string)
    # or a specific noise model that was requested upon loading of a config file.
    default_model: str

    def __init__(self):
        self.kraus_operators = {}
        self.gates = {}
        self.instruments = {}
        self.noise_models = {"": []}
        self.default_model = ""
        self.rev = 0  # Revision number, used to detect changes in the noise model

    def add_kraus_operator(self, name: str, matrices: list[ndarray]):
        self.kraus_operators[name] = matrices
        self.rev += 1

    def add_gate(self, name: str, matrix: ndarray, kraus_name: str):
        self.gates[name] = (matrix, kraus_name)
        self.rev += 1

    def update_gate_matrix(self, name: str, matrix: ndarray):
        if name in self.gates:
            (old_matrix, kraus_name) = self.gates[name]
            self.gates[name] = (matrix, kraus_name)
            self.rev += 1
        else:
            raise RuntimeError(f"Gate '{name}' not found.")

    def update_gate_noise(self, name: str, kraus_name: str):
        if name in self.gates:
            (matrix, _) = self.gates[name]
            self.gates[name] = (matrix, kraus_name)
            self.rev += 1
        else:
            raise RuntimeError(f"Gate '{name}' not found.")

    def get_noise_matrices_for_gate(self, name: str) -> list[ndarray]:
        """
        Returns the list of Kraus matrices for the specified gate.
        If the gate does not have a noise model, returns an empty list.
        """
        if name in self.gates:
            (_, noise_name) = self.gates[name]
            return self.kraus_operators.get(noise_name)
        raise RuntimeError(f"Noise for gate '{name}' not found.")

    def add_instrument(self, name: str, instrument: list[tuple[ndarray, str, str]]):
        self.instruments[name] = instrument
        self.rev += 1

    def add_noise_model(self, gates: list[str], name: str = ""):
        self.noise_models[name] = gates
        self.rev += 1

    def save_config(self, file_path):
        with open(file_path, "w") as file:
            val = {
                "krausOperators": self.kraus_operators,
                "gates": self.gates,
                "instruments": self.instruments,
                "models": self.noise_models,
            }
            json.dump(val, file, indent=2, cls=NumpyMatrixEncoder)

    def load_config(self, config_file_path):
        if not os.path.exists(config_file_path):
            raise FileNotFoundError(f"Config file '{config_file_path}' does not exist")

        with open(config_file_path, "r", encoding="utf-8") as file:
            result = json.load(file, object_hook=load_matrices_from_json)
            if (
                "krausOperators" not in result
                or "gates" not in result
                or "instruments" not in result
                or "models" not in result
            ):
                raise ValueError("Invalid noise model file")

            self.kraus_operators = result["krausOperators"]
            self.gates = result["gates"]
            self.instruments = result["instruments"]
            self.noise_models = result["models"]
            self.rev += 1

    def find_and_load_model(self, model_name: str):
        # Try to find model in the config folder
        model_config_path = os.path.abspath(
            os.path.join(
                os.path.dirname(__file__), "noise_models", model_name + ".json"
            )
        )
        if os.path.exists(model_config_path):
            self.load_config(model_config_path)
            model = self.noise_models.get("")
            if model is not None:
                self.default_model = ""
                return

        if os.path.exists(model_name):
            self.load_config(model_name)
            model = self.noise_models.get("")
            if model is not None:
                self.default_model = ""
                return

        raise RuntimeError(f"Cannot find noise model '{model_name}'")

    # This assumes a .npz file has a bunch of matrices with names like "<operator>" and "<operator>_kraus_<id>"
    # where <id> is ignored (ordering does not matter for the kraus matrices).
    # The "<operator>" is the unitary matrix and each kraus entry is one matrix in the list of kraus
    # matrices for the specified operator. This assumes the noise includes the unitary operation
    def load_npz(self, npz_file_path: str, noise_is_combined: bool):
        if not os.path.exists(npz_file_path):
            raise FileNotFoundError(f"NPZ file '{npz_file_path}' does not exist")
        data = np.load(npz_file_path)

        self.kraus_operators = {}
        self.gates = {}
        self.instruments = {}
        self.noise_models = {"": []}
        self.default_model = ""

        # Add gate unitaries and collect Kraus matrices
        raw_kraus_ops = {}
        for file in data.files:
            gate_name, match, _ = file.partition("_kraus_")
            if match == "":
                # We have the noiseless operator itself
                self.add_gate(gate_name, data[file], gate_name + "_noise")
            else:
                # We have a kraus matrix
                # Ensure the operator key exists
                if gate_name not in raw_kraus_ops:
                    raw_kraus_ops[gate_name] = []
                raw_kraus_ops[gate_name].append(data[file])

        # Extract noise from Kraus matrices if needed, build noise model
        noise_model = []
        for gate_name in raw_kraus_ops.keys():
            noise_for_gate = raw_kraus_ops[gate_name]
            if noise_is_combined:
                gate = self.gates.get(gate_name)
                if gate is None:
                    raise RuntimeError(
                        f"Cannot find noiseless matrix for gate {gate_name}"
                    )
                noise_for_gate = self.reverse_unitary_from_kraus(
                    gate[0], noise_for_gate
                )
            noise_name = gate_name + "_noise"
            self.add_kraus_operator(noise_name, noise_for_gate)
            noise_model.append(gate_name)

        self.add_noise_model(noise_model)

    def get_noisy_gates_and_instruments(self, noise_model_name: str = ""):
        model = self.noise_models.get(noise_model_name)
        if model is None:
            raise RuntimeError(f"Noise model '{noise_model_name}' not found.")
        noisy_gates = {}
        noisy_instruments = {}
        for item_name in model:
            gate = self.gates.get(item_name)
            if gate is not None:
                (noiseless_matrix, noise_name) = gate
                # NOTE: We assume that both noiseless matrix and noise only matrices are specified in the noise model.
                # We can potentially relax this condition. We don't need to require both.
                # Such relaxation is potentially dangerous - it may hide typos in dictionary keys.
                noise_matrices = self.kraus_operators.get(noise_name)
                if noise_matrices is None:
                    raise RuntimeError(
                        f"Kraus operators '{noise_name}' are not defined in the noise model."
                    )
                combined_matrices = self.apply_unitary_to_kraus(
                    noiseless_matrix, noise_matrices
                )
                noisy_gates[item_name] = combined_matrices
            else:
                instrument = self.instruments.get(item_name)
                choices = []
                if instrument is not None:
                    for noiseless_matrix, noise_name, output_string in instrument:
                        noise_matrices = self.kraus_operators.get(noise_name)
                        if noise_matrices is None:
                            raise RuntimeError(
                                f"Kraus operators '{noise_name}' are not defined in the noise model."
                            )
                        combined_matrices = self.apply_unitary_to_kraus(
                            noiseless_matrix, noise_matrices
                        )
                        choices.append((combined_matrices, output_string))
                    noisy_instruments[item_name] = choices
                else:
                    raise RuntimeError(f"Gate or instrument '{item_name}' not found.")
        return (noisy_gates, noisy_instruments)

    # Implement the below so can pass instances to lru_cache functions
    def __eq__(self, other):
        # Only compares equal to itself
        return self is other

    def __hash__(self):
        # Objects that compare equal must have the same hash
        return hash(id(self))

    ### Matrix manipulations ###

    @staticmethod
    def apply_unitary_to_kraus(U, kraus_ops):
        # If there is no noise specified, just return the unitary itself as the kraus matrices
        # TODO: extend not to require unitary. Just return kraus_ops in this case.
        if kraus_ops is None or len(kraus_ops) == 0:
            return [U]
        else:
            # Apply unitary transformation to each Kraus operator
            return [E @ U for E in kraus_ops]

    @staticmethod
    def reverse_unitary_from_kraus(
        unitary: np.ndarray, combined_kraus_ops: list[np.ndarray]
    ):
        # Given kraus operators for noise that include the unitary operation, extract just the noise operators
        U_dagger = np.conjugate(unitary.T)
        return [F @ U_dagger for F in combined_kraus_ops]


### Utility classes and method for converting NumPy matrices to and from a JSON format ###


class NumpyMatrixEncoder(json.JSONEncoder):
    """
    Use this class during JSON serialization to convert any NumPy arrays to a JSON representation
    """

    def default(self, o):
        if isinstance(o, np.ndarray):
            return [
                np.array2string(row, separator=",", max_line_width=1000000) for row in o
            ]
        return super().default(o)


def json_array_to_numpy_complex_matrix(json_arr):
    """
    Utility to convert a JSON string array to a NumPy matrix. Each strings should
    represent a row of complex numbers and be of the form "[1.+0.j, 0.+0.j]"
    """

    evalMatrix = [eval(row) for row in json_arr]
    return np.array(evalMatrix, dtype=np.complex128)


# TODO: Update to the new schema or convert after read
def load_matrices_from_json(dct: dict):
    """
    Convert the JSON representations of matrices into NumPy matrices
    """
    if "gates" in dct:
        for gName in dct["gates"]:
            (opMatrix, noise) = dct["gates"][gName]
            npArray = json_array_to_numpy_complex_matrix(opMatrix)
            dct["gates"][gName] = (npArray, noise)

    if "instruments" in dct:
        for iName in dct["instruments"]:
            choices = dct["instruments"][iName]
            upd_choices = []
            for opMatrix, noise, output in choices:
                upd_choices.append(
                    (json_array_to_numpy_complex_matrix(opMatrix), noise, output)
                )
            dct["instruments"][iName] = upd_choices

    if "krausOperators" in dct:
        # The value will be a dictionary, with each key being a kraus operator name,
        # and the value an array of arrays of strings - each an ndarray row in string form
        for krausOp in dct["krausOperators"]:
            matrixList = dct["krausOperators"][krausOp]
            newMatrices = []
            for matrix in matrixList:
                newMatrices.append(json_array_to_numpy_complex_matrix(matrix))

            dct["krausOperators"][krausOp] = newMatrices

    return dct


def amplitude_damping_kraus(gamma):
    """
    Generate Kraus operators for the amplitude damping channel.

    Parameters:
    gamma (float): Damping probability (0 <= gamma <= 1)

    Returns:
    list: Kraus operators [K0, K1] as numpy arrays
    """
    if gamma < 0 or gamma > 1:
        raise ValueError("Damping probability gamma must be between 0 and 1.")

    # Define the Kraus operators
    K0 = np.array([[1, 0], [0, np.sqrt(1 - gamma)]], dtype=np.complex128)

    K1 = np.array([[0, np.sqrt(gamma)], [0, 0]], dtype=np.complex128)

    return [K0, K1]


def rz_for_theta(theta: float) -> ndarray:
    # Top left is e^-i*theta/2, bottom right is e^i*theta/2
    return np.array(
        [[np.exp(-1j * theta / 2), 0], [0, np.exp(1j * theta / 2)]], dtype=np.complex128
    )


def create_default_noise_model() -> NoiseModel:
    noise_model = NoiseModel()

    # Add the default (noiseless) Kraus operators
    noise_model.add_kraus_operator("noise_1q", [np.eye(2, dtype=np.complex128)])
    noise_model.add_kraus_operator("noise_2q", [np.eye(4, dtype=np.complex128)])
    noise_model.add_kraus_operator("noise_3q", [np.eye(8, dtype=np.complex128)])

    # Below is equivalent to 100% amplitude damping noise, i.e. set qubit to 0 state
    noise_model.add_kraus_operator(
        "noise_reset",
        [np.array([[1 + 0j, 0], [0, 0]]), np.array([[0 + 0j, 1], [0, 0]])],
    )

    i_matrix = np.eye(2, dtype=np.complex128)
    x_matrix = np.array([[0 + 0j, 1], [1, 0]])
    y_matrix = np.array([[0 + 0j, -1j], [1j, 0]])
    z_matrix = np.array([[1 + 0j, 0], [0, -1]])

    h_matrix = np.array([[1 + 0j, 1], [1, -1]]) / np.sqrt(2)
    # np.sqrt only works on diagonal matrices
    s_matrix = np.sqrt(z_matrix)
    t_matrix = np.sqrt(s_matrix)

    # sx is the square root of x, i.e. sx * sx = x. Note np.sqrt(x) doesn't work here
    sx_matrix = np.array([[0.5 + 0.5j, 0.5 - 0.5j], [0.5 - 0.5j, 0.5 + 0.5j]])

    s_adj_matrix = np.conjugate(s_matrix.T)
    t_adj_matrix = np.conjugate(t_matrix.T)

    cx_matrix = np.array([[1 + 0j, 0, 0, 0], [0, 1, 0, 0], [0, 0, 0, 1], [0, 0, 1, 0]])
    cz_matrix = np.array([[1 + 0j, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [0, 0, 0, -1]])

    ccx_matrix = np.eye(8, dtype=np.complex128)
    ccx_matrix[6, 6] = 0
    ccx_matrix[7, 7] = 0
    ccx_matrix[6, 7] = 1
    ccx_matrix[7, 6] = 1

    # Add the default unitary operations
    noise_model.add_gate("i", i_matrix, "noise_1q")
    noise_model.add_gate("move", i_matrix, "noise_1q")
    noise_model.add_gate("x", x_matrix, "noise_1q")
    noise_model.add_gate("y", y_matrix, "noise_1q")
    noise_model.add_gate("z", z_matrix, "noise_1q")
    noise_model.add_gate("h", h_matrix, "noise_1q")
    noise_model.add_gate("s", s_matrix, "noise_1q")
    noise_model.add_gate("t", t_matrix, "noise_1q")
    noise_model.add_gate("s_adj", s_adj_matrix, "noise_1q")
    noise_model.add_gate("t_adj", t_adj_matrix, "noise_1q")
    noise_model.add_gate("sx", sx_matrix, "noise_1q")
    noise_model.add_gate("cx", cx_matrix, "noise_2q")
    noise_model.add_gate("cz", cz_matrix, "noise_2q")
    noise_model.add_gate("ccx", ccx_matrix, "noise_3q")
    # Add the reset operation, which is a gate with 100% amplitude damping noise
    noise_model.add_gate("reset", i_matrix, "noise_reset")
    # Maybe add: rx, ry, rz (how to parameterize?)
    noise_model.add_gate(
        "rz", i_matrix, "noise_1q"
    )  # Rz gate is special, needs to be handled separately
    # Also add mov for Atom (maybe just identity with noise?)

    # Add the measurement 'instruments'
    mz_matrix_0 = np.array([[1 + 0j, 0], [0, 0]])
    mz_matrix_1 = np.array([[0 + 0j, 0], [0, 1]])
    noise_model.add_instrument(
        "mz", [(mz_matrix_0, "noise_1q", "0"), (mz_matrix_1, "noise_1q", "1")]
    )

    # Add the model
    noise_model.add_noise_model(
        [
            "i",
            "move",
            "x",
            "y",
            "z",
            "h",
            "s",
            "t",
            "s_adj",
            "t_adj",
            "sx",
            "cx",
            "cz",
            "rz",
            "ccx",
            "mz",
            "reset",
        ]
    )

    return noise_model
