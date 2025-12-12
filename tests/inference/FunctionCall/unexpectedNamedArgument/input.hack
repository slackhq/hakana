<<file: __EnableUnstableFeatures('named_parameters', 'named_parameters_use')>>

function foo(int $a, named int $b, named int $c): void {}

function test(): void {
  foo(1, b=2, c=3, d=4);
}
