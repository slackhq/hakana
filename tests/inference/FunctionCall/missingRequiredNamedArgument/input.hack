<<file: __EnableUnstableFeatures('named_parameters')>>

function foo(int $a, named int $b, named int $c): void {}

function test(): void {
  foo(1, b=2);
}
