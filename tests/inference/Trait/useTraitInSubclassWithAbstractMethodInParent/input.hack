trait T {
  public function foo(): void {}
}

abstract class A {
  abstract public function foo(): void {}
}

final class B extends A {
  use T;
}