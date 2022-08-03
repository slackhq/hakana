<<__Sealed(B::class, C::class)>>
abstract class A {}
final class B extends A {}
final class C extends A {
  public function foo(): void {}
}

function handle_sealed(A $a) {
  if (!$a is B) {
    $a->foo();
  }
}