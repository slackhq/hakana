function test_variadic(string $required, int ...$optional): void {}

// This should NOT trigger TooFewArguments - variadic params are optional by nature
test_variadic("hello");

// This should also be fine
test_variadic("hello", 1, 2, 3);

function test_variadic_with_multiple_required(string $req1, int $req2, float ...$optional): void {}

// This should trigger TooFewArguments - missing $req2
test_variadic_with_multiple_required("hello");

// This should NOT trigger TooFewArguments - all required params provided
test_variadic_with_multiple_required("hello", 42);
test_variadic_with_multiple_required("hello", 42, 1.5, 2.5);