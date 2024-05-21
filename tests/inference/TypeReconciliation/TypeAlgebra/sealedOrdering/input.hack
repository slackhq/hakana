<<__Sealed(B::class, C::class)>>
class A {}
final class B extends A {
  public function foo(): int {
    return rand(0, 10);
  }
}
final class C {}

function takesA(?A $a) {
  if ($a is null || $a is C || $a->foo() > 1) {
  }
}