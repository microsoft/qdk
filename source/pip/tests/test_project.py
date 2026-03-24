# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest
import os


@pytest.fixture
def qsharp():
    import qsharp
    import qsharp._fs
    import qsharp._http

    qsharp._fs.read_file = read_file_memfs
    qsharp._fs.list_directory = list_directory_memfs
    qsharp._fs.exists = exists_memfs
    qsharp._fs.join = join_memfs
    qsharp._fs.resolve = resolve_memfs
    qsharp._http.fetch_github = fetch_github_test

    return qsharp


def test_project(qsharp) -> None:
    qsharp.init(project_root="/good")
    result = qsharp.eval("Test.ReturnsFour()")
    assert result == 4


def test_project_compile_error(qsharp) -> None:
    with pytest.raises(Exception) as excinfo:
        qsharp.init(project_root="/compile_error")
    assert str(excinfo.value).startswith("Qsc.TypeCk.TyMismatch")


def test_project_bad_qsharp_json(qsharp) -> None:
    with pytest.raises(Exception) as excinfo:
        qsharp.init(project_root="/bad_qsharp_json")
    assert str(excinfo.value).find("Failed to parse manifest") != -1


def test_project_unreadable_qsharp_json(qsharp) -> None:
    with pytest.raises(Exception) as excinfo:
        qsharp.init(project_root="/unreadable_qsharp_json")
    assert str(excinfo.value).startswith(
        "Error reading /unreadable_qsharp_json/qsharp.json."
    )


def test_project_unreadable_source(qsharp) -> None:
    with pytest.raises(Exception) as excinfo:
        qsharp.init(project_root="/unreadable_source")
    # If this seems like a silly substring to assert on, it's
    # because the error reporting code is inserting a line break
    # between "could not" and "read test.qs"
    assert str(excinfo.value).find("OSError: could not") != -1


def test_project_dependencies(qsharp) -> None:
    qsharp.init(project_root="/with_deps")
    result = qsharp.eval("Test.CallsDependency()")
    assert result == 4


def test_project_circular_dependency_error(qsharp) -> None:
    with pytest.raises(Exception) as excinfo:
        qsharp.init(project_root="/circular")
    assert str(excinfo.value).find("Circular dependency detected between") != -1


def test_github_dependency(qsharp) -> None:
    qsharp.init(project_root="/with_github_dep")
    result = qsharp.eval("Test.CallsDependency()")
    assert result == 12


def test_circuit(qsharp) -> None:
    qsharp.init(project_root="/circuit")
    result = qsharp.eval("Test.TestCircuit()")
    assert result == qsharp.Result.Zero


def test_src_package_udt(qsharp) -> None:
    qsharp.init(project_root="/src_package_udt")
    arg = qsharp.code.Test.Data(42)
    result = qsharp.run(qsharp.code.Test.Op, 1, arg)
    assert result == [42]


with open(
    os.path.join(os.path.dirname(__file__), "circuit.qsc"), "r", encoding="utf-8"
) as f:
    circuit_qsc_contents = f.read()

memfs = {
    "": {
        "good": {
            "src": {
                "test.qs": "namespace Test { operation ReturnsFour() : Int { 4 } export ReturnsFour; }",
            },
            "qsharp.json": "{}",
        },
        "src_package_udt": {
            "src": {
                "test.qs": "namespace Test { struct Data { value : Int } operation Op(data : Data) : Int { data.value } }"
            },
            "qsharp.json": "{}",
        },
        "bad_qsharp_json": {"qsharp.json": "BAD_JSON_CONTENTS"},
        "unreadable_qsharp_json": {
            "qsharp.json": OSError("could not read qsharp.json")
        },
        "unreadable_source": {
            "src": {
                "test.qs": OSError("could not read test.qs"),
            },
            "qsharp.json": "{}",
        },
        "compile_error": {
            "src": {
                "test.qs": "namespace Test { operation ReturnsFour() : Int { 4.0 } }",
            },
            "qsharp.json": "{}",
        },
        "with_deps": {
            "src": {
                "test.qs": "namespace Test { operation CallsDependency() : Int { return Foo.Test.ReturnsFour(); } }",
            },
            "qsharp.json": """
                {
                    "dependencies": {
                        "Foo": {
                            "path": "../good"
                        }
                    }
                }""",
        },
        "circular": {
            "src": {
                "test.qs": "namespace Test {}",
            },
            "qsharp.json": """
                {
                    "dependencies": {
                        "Foo": {
                            "path": "../circular"
                        }
                    }
                }""",
        },
        "with_github_dep": {
            "src": {
                "test.qs": "namespace Test { operation CallsDependency() : Int { return Foo.Test.ReturnsTwelve(); } }",
            },
            "qsharp.json": """
                {
                    "dependencies": {
                        "Foo": {
                            "github" : {
                                "owner" : "test-owner",
                                "repo" : "test-repo",
                                "ref" : "12345"
                            }
                        }
                    }
                }""",
        },
        "circuit": {
            "src": {
                "test.qs": "namespace Test {"
                "    import circuit.circuit;"
                "    operation TestCircuit() : Result {"
                "        use qs = Qubit[2];"
                "        let result = circuit(qs);"
                "        ResetAll(qs);"
                "        result"
                "    }"
                "}",
                "circuit.qsc": circuit_qsc_contents,
            },
            "qsharp.json": "{}",
        },
    }
}


def fetch_github_test(owner: str, repo: str, ref: str, path: str):
    if (
        owner == "test-owner"
        and repo == "test-repo"
        and ref == "12345"
        and path == "/qsharp.json"
    ):
        return """{ "files" : ["src/test.qs"] }"""
    if (
        owner == "test-owner"
        and repo == "test-repo"
        and ref == "12345"
        and path == "/src/test.qs"
    ):
        return "namespace Test { operation ReturnsTwelve() : Int { 12 } export ReturnsTwelve;}"
    raise Exception(f"Unexpected fetch_github call: {owner}, {repo}, {ref}, {path}")


def read_file_memfs(path):
    global memfs
    item = memfs
    for part in path.split("/"):
        if part in item:
            if isinstance(item[part], OSError):
                raise item[part]
            else:
                item = item[part]
        else:
            raise Exception("File not found: " + path)

    return (path, item)


def list_directory_memfs(dir_path):
    global memfs
    item = memfs
    for part in dir_path.split("/"):
        if part in item:
            item = item[part]
        else:
            raise Exception("Directory not found: " + dir_path)

    contents = list(
        map(
            lambda x: {
                "path": join_memfs(dir_path, x[0]),
                "entry_name": x[0],
                "type": "folder" if isinstance(x[1], dict) else "file",
            },
            item.items(),
        )
    )

    return contents


def exists_memfs(path):
    global memfs
    parts = path.split("/")
    item = memfs
    for part in parts:
        if part in item:
            item = item[part]
        else:
            return False

    return True


# The below functions force the use of `/` separators in the unit tests
# so that they function on Windows consistently with other platforms.
def join_memfs(path, *paths):
    return "/".join([path, *paths])


def resolve_memfs(base, path):
    parts = f"{base}/{path}".split("/")
    new_parts = []
    for part in parts:
        if part == ".":
            continue
        if part == "..":
            new_parts.pop()
            continue
        new_parts.append(part)
    return "/".join(new_parts)
