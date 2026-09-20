//! Helpers shared by the test files.
#![allow(dead_code)]

use litesvm::types::TransactionResult;

/// Checks that a transaction failed, and failed for the given reason, so a
/// negative test can't pass because of some unrelated mistake.
///
/// `expected` is matched against the program logs, so it can be an Anchor error
/// name ("InvalidSecret"), a framework error ("ConstraintSeeds") or a line from
/// another program ("already in use").
///
/// LiteSVM refuses two identical transactions that share a blockhash
/// ("AlreadyProcessed"), which would hide the real reason a repeat fails. Call
/// `svm.expire_blockhash()` before sending a transaction that repeats an earlier one.
pub fn assert_fails_with(res: TransactionResult, expected: &str) {
    let failed = res.expect_err("the transaction should have failed");
    let logs = failed.meta.logs.join("\n");
    assert!(
        logs.contains(expected),
        "expected the failure to mention `{expected}`, but got {:?} with logs:\n{logs}",
        failed.err
    );
}
