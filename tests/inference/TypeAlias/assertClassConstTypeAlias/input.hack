abstract class C {}

abstract class A {
  abstract const type T as C;

  public function returnC(mixed $m): C {
    return $m as this::T;
  }
}