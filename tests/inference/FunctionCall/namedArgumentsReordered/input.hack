<<file: __EnableUnstableFeatures('named_parameters', 'named_parameters_use')>>

function foo(int $a, named int $b, named string $c): void {}

function test_correct_types(): void {
  // Named arguments in different order - should work fine
  foo(1, c="hello", b=2);
}

function test_wrong_types(): void {
  // Named arguments in different order with wrong types
  // b should be int but we pass string, c should be string but we pass int
  foo(1, c=42, b="wrong");
}
