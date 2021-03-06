use proptest::prelude::*;

use super::{
    assert_threadlocal_fn_result, compile_test_fn, test_values, CompiledFunction, ConstValue,
    LangInt,
};

thread_local! {
    static EQL_FN: CompiledFunction =
        compile_test_fn("@function Int test(Int x, Int y) { return x == y }");
    static NEQ_FN: CompiledFunction =
        compile_test_fn("@function Int test(Int x, Int y) { return x != y }");
    static LT_FN: CompiledFunction =
        compile_test_fn("@function Int test(Int x, Int y) { return x < y }");
    static GT_FN: CompiledFunction =
        compile_test_fn("@function Int test(Int x, Int y) { return x > y }");
    static LTE_FN: CompiledFunction =
        compile_test_fn("@function Int test(Int x, Int y) { return x <= y }");
    static GTE_FN: CompiledFunction =
        compile_test_fn("@function Int test(Int x, Int y) { return x >= y }");
    static MULTI_EQ_FN: CompiledFunction =
        compile_test_fn("@function Int test(Int x, Int y, Int z) { return x == y == z }");
    static RANGE_TEST_FN: CompiledFunction =
        compile_test_fn("@function Int test(Int x, Int y, Int z) { return x < y <= z }");
}

// Test with random inputs.
proptest! {
    #[test]
    fn proptest_comparisons(x: LangInt, y: LangInt) {
        test_comparisons(x, y);
    }

    #[test]
    fn proptest_multi_comparisons(x: LangInt, y: LangInt, z: LangInt) {
        test_multi_comparisons(x, y, z);
    }
}

// And make sure to cover several corner cases (e.g. division by zero).
#[test]
fn test_comparisons_corner_cases() {
    for (&x, &y) in iproduct!(test_values(), test_values()) {
        println!("Testing comparisons with inputs {:?}", (x, y));
        test_comparisons(x, y);
    }
}
#[test]
fn test_multi_comparisons_corner_cases() {
    for (&x, &y, &z) in iproduct!(test_values(), test_values(), test_values()) {
        println!("Testing combined comparisons with inputs {:?}", (x, y, z));
        test_multi_comparisons(x, y, z);
    }
}

fn test_comparisons(x: LangInt, y: LangInt) {
    let mut args = [ConstValue::Int(x), ConstValue::Int(y)];

    // Equal
    let expected = Ok(ConstValue::Int((x == y).into()));
    assert_threadlocal_fn_result(&EQL_FN, &mut args, expected);

    // Not equal
    let expected = Ok(ConstValue::Int((x != y).into()));
    assert_threadlocal_fn_result(&NEQ_FN, &mut args, expected);

    // Less than
    let expected = Ok(ConstValue::Int((x < y).into()));
    assert_threadlocal_fn_result(&LT_FN, &mut args, expected);

    // Greater than
    let expected = Ok(ConstValue::Int((x > y).into()));
    assert_threadlocal_fn_result(&GT_FN, &mut args, expected);

    // Less than or equal
    let expected = Ok(ConstValue::Int((x <= y).into()));
    assert_threadlocal_fn_result(&LTE_FN, &mut args, expected);

    // Greater than or equal
    let expected = Ok(ConstValue::Int((x >= y).into()));
    assert_threadlocal_fn_result(&GTE_FN, &mut args, expected);
}
fn test_multi_comparisons(x: LangInt, y: LangInt, z: LangInt) {
    let mut args = [ConstValue::Int(x), ConstValue::Int(y), ConstValue::Int(z)];

    // Equal
    let expected = Ok(ConstValue::Int((x == y && y == z).into()));
    assert_threadlocal_fn_result(&MULTI_EQ_FN, &mut args, expected);

    // Range
    let expected = Ok(ConstValue::Int((x < y && y <= z).into()));
    assert_threadlocal_fn_result(&RANGE_TEST_FN, &mut args, expected);
}
