from pathlib import Path

FIXTURES = Path(__file__).with_name("fixtures")


def physical_qodec():
    """The repetition fixture's physical layer, with instructions named for QIR."""
    import qodec

    codec = qodec.Qodec.load(str(FIXTURES / "repetition3.qodec.yaml"))
    isa = codec.layers[-1].instruction_set
    names = {"R": "reset", "M": "m", "CX": "cx", "rotate_z": "rz"}
    instructions = {}
    for mnemonic, instruction in isa.instructions.items():
        name = names.get(mnemonic, mnemonic.lower())
        instructions[name] = qodec.Instruction(
            name,
            description=instruction.description,
            inputs=instruction.inputs,
            outputs=instruction.outputs,
            flags=instruction.flags,
            parameters=instruction.parameters,
            action=instruction.action,
        )
    return qodec.Qodec(
        [
            qodec.Layer(
                qodec.InstructionSet(
                    isa.name, blocks=isa.blocks, instructions=instructions
                )
            )
        ]
    )
