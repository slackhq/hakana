function test_function(string $param1, int $param2): void {}

// This should trigger TooFewArguments - missing param2
test_function("hello");

// This should also trigger TooFewArguments - missing both params
test_function();