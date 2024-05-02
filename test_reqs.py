"""
Testing program to send a bunch of reqs
"""
import code_exec_reqs


CODE_PASS = """
assert True
"""


CODE_FAIL = """
assert False, "This should fail"
"""

pass_req = code_exec_reqs.exec_test("http://127.0.0.1:8000", CODE_PASS, "")
print(pass_req)

fail_req = code_exec_reqs.exec_test("http://127.0.0.1:8000", CODE_FAIL, "")
print(fail_req)

pass_req = code_exec_reqs.exec_test_multipl_e(
    "http://127.0.0.1:8000", CODE_PASS, "", "python"
)
print(pass_req)

fail_req = code_exec_reqs.exec_test_multipl_e(
    "http://127.0.0.1:8000", CODE_FAIL,  "", "python"
)
print(fail_req)


# batched
codes = [
    CODE_PASS,
    CODE_FAIL,
    CODE_PASS,
    CODE_FAIL,
    CODE_PASS,
]
tests = ["" for _ in range(len(codes))]
batched_req = code_exec_reqs.exec_test_batched(
    "http://127.0.0.1:8000", codes, tests)
print(batched_req)
assert len(batched_req) == len(codes)
