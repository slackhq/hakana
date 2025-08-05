function test_function_with_defaults(string $param1, int $param2 = 42, string $param3 = "default"): void {}

// These should NOT trigger TooFewArguments because missing parameters have defaults
test_function_with_defaults("hello");
test_function_with_defaults("hello", 123);

function test_mixed_params(string $required, int $optional = 10, string $required2): void {}

// This should trigger TooFewArguments because $required2 has no default
test_mixed_params("hello");

// This should also trigger TooFewArguments because $required2 has no default
test_mixed_params("hello", 123);

// This should NOT trigger TooFewArguments - all required params provided
test_mixed_params("hello", 123, "world");
