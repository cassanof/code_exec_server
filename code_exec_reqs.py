from typing import List, Optional, Tuple
import requests
import json
import threading


def exec_test(server, code, test, timeout=30) -> Tuple[bool, str]:
    """
    Executes a test against a code snippet.
    Produces true if the test passes, false otherwise.
    Also returns the output of the code (sterr if it fails, stdout if it passes).

    You can set test to an empty string if you want to execute the code without any tests
    and just check if it runs without errors.
    """
    code_with_tests = code + "\n\n" + test
    data = json.dumps({"code": code_with_tests})
    try:
        r = requests.post(
            server + "/py_exec",
            data=data,
            timeout=timeout)
        lines = r.text.split("\n")
        resp = lines[0]
        outs = "\n".join(lines[1:])
        assert resp == "0" or resp == "1"
        return resp == "0", outs
    except Exception as e:
        print(e)
        return False, "Failed to execute program"


def exec_test_batched(server, codes, tests, timeout=30) -> List[Tuple[bool, str]]:
    """
    Executes a batch of tests against a batch of code snippets using threading.
    """
    threads = []
    results: List[Optional[Tuple[bool, str]]] = [None] * len(codes)

    def exec_test_threaded(i, code, test):
        results[i] = exec_test(server, code, test, timeout)

    for i, (code, test) in enumerate(zip(codes, tests)):
        t = threading.Thread(target=exec_test_threaded, args=(i, code, test))
        threads.append(t)
        t.start()

    for t in threads:
        t.join()

    assert all(r is not None for r in results)
    return results


def run_coverage(server, code, tests):
    """
    Executes a code snippet and tests it with a set of tests,
    then returns the coverage percentage using coverage.py.
    """
    tests_str = "\n".join(tests)
    code_with_tests = code + "\n\n" + tests_str
    data = json.dumps({"code": code_with_tests})
    timeout = 80  # 60 for run, 10 for report, 10 for safety
    try:
        r = requests.post(
            server + "/py_coverage",
            data=data,
            timeout=timeout,
        )
        return int(r.text)
    except Exception as e:
        print(e)
        return -3


def run_coverage_batched(server, codes, tests):
    """
    Executes a batch of code snippets and tests them with a set of tests,
    then returns the coverage percentage using coverage.py.
    """
    threads = []
    results: List[Optional[int]] = [None] * len(codes)

    def run_coverage_threaded(i, code, test):
        results[i] = run_coverage(server, code, test)

    for i, (code, test) in enumerate(zip(codes, tests)):
        t = threading.Thread(target=run_coverage_threaded,
                             args=(i, code, test))
        threads.append(t)
        t.start()

    for t in threads:
        t.join()

    assert all(r is not None for r in results)
    return results
