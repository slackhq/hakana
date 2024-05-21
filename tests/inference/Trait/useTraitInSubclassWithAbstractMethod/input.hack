trait T {
  abstract public function foo(): void;
}

abstract class A {
  public function foo(): void {}
}

final class B extends A {
  use T;
}