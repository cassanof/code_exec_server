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
