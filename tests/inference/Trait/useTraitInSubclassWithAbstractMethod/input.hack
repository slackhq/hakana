trait T {
  abstract public function foo(): void;
}

abstract class A {
  public function foo(): void {}
}

class B extends A {
  use T;
}